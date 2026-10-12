//! Expression evaluation, string utilities, and arithmetic parsing.

use std::collections::HashMap;

use super::{DefineEntry, ParamSpec, Value};

/// Remove surrounding quotes from a value string.
pub(super) fn unquote(s: &str) -> String {
    let s = s.trim();
    if is_quoted_literal(s) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

pub(super) fn is_quoted_literal(s: &str) -> bool {
    s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
}

pub(super) fn normalize_param_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if is_quoted_literal(trimmed) {
        let inner = unquote(trimmed);
        if requires_round_trip_quotes(&inner) {
            return trimmed.to_string();
        }
        return inner;
    }
    trimmed.to_string()
}

pub(super) fn requires_round_trip_quotes(value: &str) -> bool {
    value.contains(',') || value.contains('(') || value.contains(')')
}

/// Expand built-in functions in normal lines.
///
/// `%newline()` and `%n()` produce PlantUML's hidden newline placeholder,
/// which downstream parsers/renderers interpret according to Display semantics.
pub(super) fn expand_builtins(line: &str) -> String {
    let mut result = line.to_string();

    // %newline() / %n() → U+E100 placeholder (matches Java PlantUML)
    let nl = crate::NEWLINE_CHAR.to_string();
    result = result.replace("%newline()", &nl);
    result = result.replace("%n()", &nl);

    // %chr(N) — e.g. %chr(65) -> 'A'
    while let Some(start) = result.find("%chr(") {
        let Some(end) = result[start..].find(')') else {
            break;
        };
        let inner = &result[start + 5..start + end];
        let Ok(n) = inner.trim().parse::<u32>() else {
            break;
        };
        let ch = char::from_u32(n).unwrap_or('?');
        result = format!("{}{}{}", &result[..start], ch, &result[start + end + 1..]);
    }

    // %strlen(s), %substr — skip gracefully
    // %true() -> "true", %false() -> "false"
    result = result.replace("%true()", "true");
    result = result.replace("%false()", "false");

    result
}

pub(super) fn expand_expression_builtins(line: &str) -> String {
    let mut result = line.to_string();

    while let Some(start) = result.find("%chr(") {
        let Some(end) = result[start..].find(')') else {
            break;
        };
        let inner = &result[start + 5..start + end];
        let Ok(n) = inner.trim().parse::<u32>() else {
            break;
        };
        let ch = char::from_u32(n).unwrap_or('?');
        let literal = match ch {
            '"' => "'\"'".to_string(),
            '\'' => "\"'\"".to_string(),
            _ => format!("\"{ch}\""),
        };
        result = format!(
            "{}{}{}",
            &result[..start],
            literal,
            &result[start + end + 1..]
        );
    }

    // Java's %newline() returns the U+E100 placeholder character (Jaws.BLOCK_E1_NEWLINE),
    // not a literal '\n'. The placeholder lets the value flow through line-based
    // parsing without splitting, while still being interpreted as a soft line break
    // by downstream renderers (Display.create handles U+E100).
    let nl = crate::NEWLINE_CHAR.to_string();
    let nl_quoted = format!("\"{nl}\"");
    result = result.replace("%newline()", &nl_quoted);
    result = result.replace("%n()", &nl_quoted);
    result = result.replace("%true()", "true");
    result = result.replace("%false()", "false");
    result
}

/// Find the macro name as a whole word (not a substring of a longer identifier).
/// Returns the byte offset of the match, or None.
/// Java Define.apply2(): replace all occurrences of `name` at word boundaries
/// with `replacement`.  Matches Java's `\b<name>\b` regex semantics.
pub(super) fn replace_word_boundary(haystack: &str, name: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(haystack.len());
    let mut search_from = 0;
    let bytes = haystack.as_bytes();
    while let Some(pos) = haystack[search_from..].find(name) {
        let abs_pos = search_from + pos;
        let end_pos = abs_pos + name.len();
        // Check word boundary before
        let before_ok = if abs_pos == 0 {
            true
        } else {
            let prev = bytes[abs_pos - 1];
            !prev.is_ascii_alphanumeric() && prev != b'_'
        };
        // Check word boundary after
        let after_ok = if end_pos >= bytes.len() {
            true
        } else {
            let next = bytes[end_pos];
            !next.is_ascii_alphanumeric() && next != b'_'
        };
        if before_ok && after_ok {
            result.push_str(&haystack[search_from..abs_pos]);
            result.push_str(replacement);
            search_from = end_pos;
        } else {
            // Not a word boundary, copy the character and continue. Step a
            // whole one: a needle can begin with a multi-byte character, and
            // `abs_pos + 1` would then split it.
            let step = haystack[abs_pos..].chars().next().map_or(1, char::len_utf8);
            result.push_str(&haystack[search_from..abs_pos + step]);
            search_from = abs_pos + step;
        }
    }
    result.push_str(&haystack[search_from..]);
    result
}

fn find_whole_word(haystack: &str, name: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(pos) = haystack[search_from..].find(name) {
        let abs_pos = search_from + pos;
        // Check character before: must not be alphanumeric or underscore
        let before_ok = if abs_pos == 0 {
            true
        } else {
            let prev = haystack.as_bytes()[abs_pos - 1];
            !prev.is_ascii_alphanumeric() && prev != b'_'
        };
        // Character after name is already checked by the caller (must be '('),
        // but for safety ensure it's not part of an identifier
        if before_ok {
            return Some(abs_pos);
        }
        // Step a whole character: `name` can begin with a multi-byte one, and
        // `abs_pos + 1` would then split it for the next `haystack[..]`.
        search_from = abs_pos + haystack[abs_pos..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// Expand a parameterised `!define` macro within a line.
pub(super) fn expand_parameterised_define(line: &str, name: &str, entry: &DefineEntry) -> String {
    use std::sync::OnceLock;
    static CONCAT_RE: OnceLock<regex::Regex> = OnceLock::new();
    let mut result = line.to_string();
    let concat_re = CONCAT_RE.get_or_init(|| regex::Regex::new(r"\s*##\s*").unwrap());

    while let Some(start) = find_whole_word(&result, name) {
        let after_name = start + name.len();
        let rest = &result[after_name..];

        if !rest.starts_with('(') {
            break;
        }

        // Find matching ')'
        let mut depth = 0;
        let mut end = 0;
        let mut found = false;
        for (idx, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = idx;
                        found = true;
                        break;
                    }
                }
                _ => {}
            }
        }
        if !found {
            break;
        }

        let args_str = &rest[1..end];
        let args = split_args(args_str);

        // Substitute parameters in body
        let mut expanded_body = entry.body.clone();
        for (i, param) in entry.params.iter().enumerate() {
            let arg = args.get(i).map_or("", |s| s.trim());
            // Handle default values in param definition: `param = "default"`
            let (param_name, default) = if let Some(eq) = param.find('=') {
                let pn = param[..eq].trim();
                let dv = unquote(param[eq + 1..].trim());
                (pn, Some(dv))
            } else {
                (param.trim(), None)
            };

            let value = if arg.is_empty() {
                default.unwrap_or_default()
            } else {
                unquote(arg)
            };

            expanded_body = expanded_body.replace(param_name, &value);
        }

        // Handle ## token concatenation: remove `##` and join adjacent text
        if expanded_body.contains("##") {
            expanded_body = concat_re.replace_all(&expanded_body, "").to_string();
        }

        result = format!("{}{}{}", &result[..start], expanded_body, &rest[end + 1..]);
    }

    result
}

/// Split comma-separated arguments, respecting parentheses and quotes.
pub(super) fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut quote_char = None;

    for ch in s.chars() {
        match ch {
            '"' | '\'' if quote_char.is_none() => {
                quote_char = Some(ch);
                current.push(ch);
            }
            ch if quote_char == Some(ch) => {
                quote_char = None;
                current.push(ch);
            }
            '(' if quote_char.is_none() => {
                depth += 1;
                current.push(ch);
            }
            ')' if quote_char.is_none() => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 && quote_char.is_none() => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() || !args.is_empty() {
        args.push(trimmed);
    }

    args
}

pub(super) fn parse_param_spec(raw: &str) -> ParamSpec {
    let raw = raw.trim();
    if raw.is_empty() {
        return ParamSpec {
            name: String::new(),
            default: None,
        };
    }

    if let Some((name, default)) = split_top_level_once(raw, '=') {
        ParamSpec {
            name: name.trim().to_string(),
            default: Some(default.trim().to_string()),
        }
    } else {
        ParamSpec {
            name: raw.to_string(),
            default: None,
        }
    }
}

pub(super) fn parse_call_arguments(args: &[String]) -> (Vec<String>, HashMap<String, String>) {
    let mut positional = Vec::new();
    let mut keyword = HashMap::new();

    for arg in args {
        if let Some((name, value)) = split_top_level_once(arg, '=') {
            let key = name.trim();
            if !key.is_empty()
                && key
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '$'))
            {
                keyword.insert(key.to_string(), value.trim().to_string());
                continue;
            }
        }
        positional.push(arg.clone());
    }

    (positional, keyword)
}

pub(super) fn split_top_level_once(s: &str, needle: char) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    let mut quote_char = None;
    let mut escaped = false;

    for (idx, ch) in s.char_indices() {
        if let Some(active_quote) = quote_char {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote_char = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote_char = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if ch == needle && depth == 0 => {
                let next = idx + ch.len_utf8();
                return Some((&s[..idx], &s[next..]));
            }
            _ => {}
        }
    }

    None
}

pub(super) fn split_top_level_operator<'a>(s: &'a str, operator: &str) -> Option<Vec<&'a str>> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quote_char = None;
    let mut escaped = false;
    let mut start = 0usize;
    let bytes = s.as_bytes();
    let op_bytes = operator.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        let ch = s[idx..].chars().next()?;
        let ch_len = ch.len_utf8();

        if let Some(active_quote) = quote_char {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote_char = None;
            }
            idx += ch_len;
            continue;
        }

        match ch {
            '"' | '\'' => quote_char = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }

        if depth == 0
            && idx + op_bytes.len() <= bytes.len()
            && &bytes[idx..idx + op_bytes.len()] == op_bytes
        {
            parts.push(s[start..idx].trim());
            idx += op_bytes.len();
            start = idx;
            continue;
        }

        idx += ch_len;
    }

    if parts.is_empty() {
        None
    } else {
        parts.push(s[start..].trim());
        Some(parts)
    }
}

pub(super) fn strip_wrapping_parens(s: &str) -> Option<&str> {
    let s = s.trim();
    if !(s.starts_with('(') && s.ends_with(')')) {
        return None;
    }

    let mut depth = 0usize;
    let mut quote_char = None;
    let mut escaped = false;

    for (idx, ch) in s.char_indices() {
        if let Some(active_quote) = quote_char {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote_char = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote_char = Some(ch),
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && idx + ch.len_utf8() != s.len() {
                    return None;
                }
            }
            _ => {}
        }
    }

    if depth == 0 {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

pub(super) fn find_matching_call_end(rest: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote_char = None;
    let mut escaped = false;

    for (idx, ch) in rest.char_indices() {
        if let Some(active_quote) = quote_char {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote_char = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote_char = Some(ch),
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }

    None
}

pub(super) fn replace_named_call<F>(line: &str, name: &str, mut replacer: F) -> Option<String>
where
    F: FnMut(&str) -> Option<String>,
{
    let call_start = find_named_call_start(line, name)?;
    let after_name = call_start + name.len();
    let rest = &line[after_name..];
    if !rest.starts_with('(') {
        return None;
    }

    let end = find_matching_call_end(rest)?;
    let args_str = &rest[1..end];
    let replacement = replacer(args_str)?;

    let mut out = String::new();
    out.push_str(&line[..call_start]);
    out.push_str(&replacement);
    out.push_str(&rest[end + 1..]);
    Some(out)
}

pub(super) fn find_named_call_start(line: &str, name: &str) -> Option<usize> {
    for (idx, _) in line.match_indices(name) {
        if idx > 0 && !name.starts_with('$') && !name.starts_with('%') {
            let prev = line[..idx].chars().next_back()?;
            if prev.is_alphanumeric() || matches!(prev, '_' | '$' | '%') {
                continue;
            }
        }
        let after_name = idx + name.len();
        if line[after_name..].starts_with('(') {
            return Some(idx);
        }
    }
    None
}

pub(super) fn is_variable_boundary_end(line: &str, end: usize) -> bool {
    match line[end..].chars().next() {
        Some(ch) => !(ch.is_alphanumeric() || ch == '_'),
        None => true,
    }
}

/// Parse an array literal like `[1, 2, 3]` or `["a", "b", "c"]` into strings.
///
/// Returns `None` if the string doesn't look like a bracket array.
/// Kept for backward compatibility; new code should prefer `parse_array_values`.
pub(super) fn parse_array(s: &str) -> Option<Vec<String>> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    if inner.trim().is_empty() {
        return Some(vec![]);
    }
    let items: Vec<String> = inner
        .split(',')
        .map(|item| {
            let item = item.trim();
            unquote(item)
        })
        .collect();
    Some(items)
}

/// Parse an array literal into typed `Value` items.
///
/// Returns `None` if the string doesn't look like a bracket array.
pub(super) fn parse_array_values(s: &str) -> Option<Vec<Value>> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    if inner.trim().is_empty() {
        return Some(vec![]);
    }
    let items: Vec<Value> = inner
        .split(',')
        .map(|item| {
            let item = item.trim();
            let unquoted = unquote(item);
            // If the original was quoted, treat as string even if it looks numeric
            if (item.starts_with('"') && item.ends_with('"'))
                || (item.starts_with('\'') && item.ends_with('\''))
            {
                Value::Str(unquoted)
            } else {
                Value::parse_from(&unquoted)
            }
        })
        .collect();
    Some(items)
}

/// Try to evaluate a string as an arithmetic expression.
///
/// Supports `+`, `-`, `*`, `/`, `%` on numeric (f64) operands.
/// Returns `None` if the expression is not purely arithmetic.
/// The result is formatted as an integer if it has no fractional part.
pub(super) fn try_eval_arithmetic(s: &str) -> Option<String> {
    let s = s.trim();

    // Quick check: must contain at least one operator to be arithmetic
    // (but not just a plain number — we leave those as-is).
    let has_operator = s.chars().enumerate().any(|(i, c)| {
        if "+-*/%".contains(c) {
            // Leading sign is not an operator
            if (c == '+' || c == '-') && i == 0 {
                return false;
            }
            return true;
        }
        false
    });
    if !has_operator {
        return None;
    }

    // Tokenise: numbers and operators
    let result = eval_arith_expr(s)?;

    // Format: prefer integer representation when possible
    if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
        Some(format!("{}", result as i64))
    } else {
        Some(format!("{result}"))
    }
}

pub(super) fn try_eval_concat_expr(s: &str) -> Option<String> {
    let parts = split_concat_parts(s)?;
    if parts.len() < 2 {
        return None;
    }

    let mut out = String::new();
    for part in parts {
        let trimmed = trim_concat_part(&part);
        if trimmed.is_empty() {
            continue;
        }
        match trimmed {
            "%newline()" | "%n()" => out.push('\n'),
            _ if ((trimmed.starts_with('"') && trimmed.ends_with('"'))
                || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
                && trimmed.len() >= 2 =>
            {
                out.push_str(&trimmed[1..trimmed.len() - 1]);
            }
            _ => out.push_str(trimmed),
        }
    }
    Some(out)
}

pub(super) fn split_concat_parts(s: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote_char = None;
    let mut escaped = false;
    let mut paren_depth = 0usize;
    let mut saw_plus = false;

    for ch in s.chars() {
        if let Some(active_quote) = quote_char {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote_char = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                quote_char = Some(ch);
                current.push(ch);
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            '+' if paren_depth == 0 => {
                saw_plus = true;
                parts.push(current);
                current = String::new();
            }
            _ => current.push(ch),
        }
    }

    if !saw_plus || quote_char.is_some() || paren_depth != 0 {
        return None;
    }

    parts.push(current);
    Some(parts)
}

pub(super) fn trim_concat_part(s: &str) -> &str {
    s.trim_matches(|ch| matches!(ch, ' ' | '\t'))
}

/// Simple recursive-descent arithmetic evaluator.
///
/// Grammar:
///   expr   = term (('+' | '-') term)*
///   term   = factor (('*' | '/' | '%') factor)*
///   factor = ['-' | '+'] atom
///   atom   = number | '(' expr ')'
pub(super) fn eval_arith_expr(s: &str) -> Option<f64> {
    let tokens = tokenize_arith(s)?;
    let mut pos = 0;
    let result = parse_expr(&tokens, &mut pos)?;
    if pos == tokens.len() {
        Some(result)
    } else {
        None // leftover tokens — not a valid expression
    }
}

#[derive(Debug, Clone)]
pub(super) enum ArithToken {
    Num(f64),
    Op(char),
    LParen,
    RParen,
}

pub(super) fn tokenize_arith(s: &str) -> Option<Vec<ArithToken>> {
    let mut tokens = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '(' {
            tokens.push(ArithToken::LParen);
            chars.next();
        } else if c == ')' {
            tokens.push(ArithToken::RParen);
            chars.next();
        } else if "+-*/%".contains(c) {
            // Determine if this is a unary sign or a binary operator
            let is_unary = tokens.is_empty()
                || matches!(tokens.last(), Some(ArithToken::Op(_) | ArithToken::LParen));
            if is_unary && (c == '+' || c == '-') {
                // Parse as part of the number
                let sign = if c == '-' { -1.0 } else { 1.0 };
                chars.next();
                // skip whitespace between sign and number
                while let Some(&ws) = chars.peek() {
                    if ws.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let num = read_number(&mut chars)?;
                tokens.push(ArithToken::Num(sign * num));
            } else {
                tokens.push(ArithToken::Op(c));
                chars.next();
            }
        } else if c.is_ascii_digit() || c == '.' {
            let num = read_number(&mut chars)?;
            tokens.push(ArithToken::Num(num));
        } else {
            // Non-arithmetic character — this is not a pure arithmetic expression
            return None;
        }
    }

    Some(tokens)
}

pub(super) fn read_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<f64> {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' {
            s.push(c);
            chars.next();
        } else {
            break;
        }
    }
    s.parse::<f64>().ok()
}

pub(super) fn parse_expr(tokens: &[ArithToken], pos: &mut usize) -> Option<f64> {
    let mut left = parse_term(tokens, pos)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            ArithToken::Op('+') => {
                *pos += 1;
                left += parse_term(tokens, pos)?;
            }
            ArithToken::Op('-') => {
                *pos += 1;
                left -= parse_term(tokens, pos)?;
            }
            _ => break,
        }
    }
    Some(left)
}

pub(super) fn parse_term(tokens: &[ArithToken], pos: &mut usize) -> Option<f64> {
    let mut left = parse_factor(tokens, pos)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            ArithToken::Op('*') => {
                *pos += 1;
                left *= parse_factor(tokens, pos)?;
            }
            ArithToken::Op('/') => {
                *pos += 1;
                let right = parse_factor(tokens, pos)?;
                if right == 0.0 {
                    return None; // division by zero
                }
                left /= right;
            }
            ArithToken::Op('%') => {
                *pos += 1;
                let right = parse_factor(tokens, pos)?;
                if right == 0.0 {
                    return None;
                }
                left %= right;
            }
            _ => break,
        }
    }
    Some(left)
}

pub(super) fn parse_factor(tokens: &[ArithToken], pos: &mut usize) -> Option<f64> {
    if *pos >= tokens.len() {
        return None;
    }
    match &tokens[*pos] {
        ArithToken::Num(n) => {
            let v = *n;
            *pos += 1;
            Some(v)
        }
        ArithToken::LParen => {
            *pos += 1;
            let v = parse_expr(tokens, pos)?;
            if *pos < tokens.len() && matches!(&tokens[*pos], ArithToken::RParen) {
                *pos += 1;
            } else {
                return None; // missing closing paren
            }
            Some(v)
        }
        _ => None,
    }
}

/// Extract the argument string from a function call like `%func("arg")`.
pub(super) fn extract_func_arg<'a>(expr: &'a str, func_name: &str) -> Option<&'a str> {
    let start = expr.find(func_name)?;
    let rest = &expr[start + func_name.len()..];
    if !rest.starts_with('(') {
        return None;
    }
    let close = rest.rfind(')')?;
    Some(&rest[1..close])
}
