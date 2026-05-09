//! d2-parser: Streaming lexer + recursive descent parser for the d2 language.
//!
//! Ported from Go d2parser/parse.go.

use crate::ast::*;
use std::fmt;

// ---------------------------------------------------------------------------
// ParseError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ParseError {
    pub errors: Vec<Error>,
}

impl ParseError {
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, err) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{}", err)?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a d2 source string and return the root `Map` AST plus any errors.
///
/// Even when errors are returned, the `Map` represents a best-effort partial AST.
pub fn parse(path: &str, input: &str) -> (Map, Option<ParseError>) {
    let mut p = Parser::new(path, input);
    let m = p.parse_map(true);
    if p.errors.is_empty() {
        (m, None)
    } else {
        (m, Some(ParseError { errors: p.errors }))
    }
}

/// Parse a single key path (e.g. `a.b.c`).
pub fn parse_key(key: &str) -> Result<KeyPath, String> {
    let mut p = Parser::new("", key);
    match p.parse_key_path() {
        Some(k) if p.errors.is_empty() => Ok(k),
        Some(_) => Err(format!(
            "failed to parse key {:?}: {}",
            key,
            ParseError { errors: p.errors }
        )),
        None => Err(format!("empty key: {:?}", key)),
    }
}

/// Parse a single map key (e.g. `x -> y: label`).
pub fn parse_map_key(input: &str) -> Result<Key, String> {
    let mut p = Parser::new("", input);
    match p.parse_map_key() {
        Some(k) if p.errors.is_empty() => Ok(k),
        Some(_) => Err(format!(
            "failed to parse map key {:?}: {}",
            input,
            ParseError { errors: p.errors }
        )),
        None => Err(format!("empty map key: {:?}", input)),
    }
}

// ---------------------------------------------------------------------------
// Internal parser
// ---------------------------------------------------------------------------

struct Parser {
    path: String,
    /// Full source as chars for random access.
    chars: Vec<char>,
    /// Current index into `chars`.
    cursor: usize,
    /// Current position (line, column, byte).
    pos: Position,

    errors: Vec<Error>,

    in_edge_group: bool,
    depth: usize,
}

impl Parser {
    fn new(path: &str, input: &str) -> Self {
        // Strip UTF-8 BOM if present.
        let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
        Self {
            path: path.to_string(),
            chars: input.chars().collect(),
            cursor: 0,
            pos: Position::default(),
            errors: Vec::new(),
            in_edge_group: false,
            depth: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Character I/O helpers
    // -----------------------------------------------------------------------

    fn eof(&self) -> bool {
        self.cursor >= self.chars.len()
    }

    /// Read the next char, advancing position.
    fn read(&mut self) -> Option<char> {
        if self.cursor >= self.chars.len() {
            return None;
        }
        let ch = self.chars[self.cursor];
        self.cursor += 1;
        self.pos = self.pos.advance(ch);
        Some(ch)
    }

    /// Peek at the next char without advancing.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).copied()
    }

    /// Peek at the next n chars as a string.
    fn peek_n(&self, n: usize) -> Option<String> {
        if self.cursor + n > self.chars.len() {
            return None;
        }
        Some(self.chars[self.cursor..self.cursor + n].iter().collect())
    }

    /// Unread the last char, rewinding position.
    fn unread(&mut self, ch: char) {
        self.pos = self.pos.subtract(ch);
        self.cursor -= 1;
    }

    /// Read the next non-space character (consuming spaces).
    fn read_not_space(&mut self) -> Option<char> {
        loop {
            let ch = self.read()?;
            if !ch.is_whitespace() {
                return Some(ch);
            }
        }
    }

    /// Peek at the next non-space character, returning it and how many newlines were crossed.
    fn peek_not_space(&self) -> (Option<char>, usize) {
        let mut newlines = 0;
        let mut i = self.cursor;
        loop {
            if i >= self.chars.len() {
                return (None, newlines);
            }
            let ch = self.chars[i];
            i += 1;
            if ch.is_whitespace() {
                if ch == '\n' {
                    newlines += 1;
                }
                continue;
            }
            return (Some(ch), newlines);
        }
    }

    /// Skip whitespace, advance cursor and position, return count of newlines skipped.
    fn skip_whitespace(&mut self) -> usize {
        let mut newlines = 0;
        while self.cursor < self.chars.len() {
            let ch = self.chars[self.cursor];
            if !ch.is_whitespace() {
                break;
            }
            self.cursor += 1;
            self.pos = self.pos.advance(ch);
            if ch == '\n' {
                newlines += 1;
            }
        }
        newlines
    }

    /// Skip whitespace but not newlines.
    #[allow(dead_code)]
    fn skip_space_no_newline(&mut self) {
        while self.cursor < self.chars.len() {
            let ch = self.chars[self.cursor];
            if !ch.is_whitespace() || ch == '\n' {
                break;
            }
            self.cursor += 1;
            self.pos = self.pos.advance(ch);
        }
    }

    /// Record a parse error.
    fn errorf(&mut self, start: Position, end: Position, msg: String) {
        let r = Range::new(self.path.clone(), start, end);
        let message = format!("{}: {}", r, msg);
        self.errors.push(Error { range: r, message });
    }

    // -----------------------------------------------------------------------
    // Save/restore for lookahead patterns
    // -----------------------------------------------------------------------

    fn save(&self) -> (usize, Position) {
        (self.cursor, self.pos)
    }

    fn restore(&mut self, state: (usize, Position)) {
        self.cursor = state.0;
        self.pos = state.1;
    }

    // -----------------------------------------------------------------------
    // parse_map (root and nested)
    // -----------------------------------------------------------------------

    fn parse_map(&mut self, is_file_map: bool) -> Map {
        let mut m = Map {
            range: Range::new(
                self.path.clone(),
                if is_file_map {
                    self.pos
                } else {
                    self.pos.subtract('{')
                },
                Position::default(),
            ),
            nodes: Vec::new(),
        };

        if !is_file_map {
            self.depth += 1;
        }

        loop {
            let r = self.read_not_space();
            let Some(r) = r else {
                if !is_file_map {
                    let end = self.pos;
                    self.errorf(m.range.start, end, "maps must be terminated with }".into());
                }
                m.range.end = self.pos;
                if !is_file_map {
                    self.depth -= 1;
                }
                return m;
            };

            match r {
                ';' => continue,
                '}' => {
                    if is_file_map {
                        let start = self.pos.subtract(r);
                        let end = self.pos;
                        self.errorf(
                            start,
                            end,
                            "unexpected map termination character } in file map".into(),
                        );
                        continue;
                    }
                    m.range.end = self.pos;
                    self.depth -= 1;
                    return m;
                }
                _ => {}
            }

            if let Some(n) = self.parse_map_node(r) {
                let is_block_comment = matches!(&n, MapNode::BlockComment(_));
                m.nodes.push(n);

                if is_block_comment {
                    continue;
                }
            }

            // Consume unexpected trailing text on the same line.
            let after = self.pos;
            loop {
                let (ch, newlines) = self.peek_not_space();
                match ch {
                    None => break,
                    Some(c) if newlines != 0 || c == ';' || c == '}' || c == '#' => break,
                    Some(_) => {
                        // consume
                        self.skip_whitespace();
                        let _ = self.read();
                    }
                }
            }
            if after != self.pos {
                let end = self.pos;
                self.errorf(after, end, "unexpected text after node".into());
            }
        }
    }

    // -----------------------------------------------------------------------
    // parse_map_node
    // -----------------------------------------------------------------------

    fn parse_map_node(&mut self, r: char) -> Option<MapNode> {
        match r {
            '#' => {
                return Some(MapNode::Comment(self.parse_comment()));
            }
            '"' => {
                // Check for block comment """
                if let Some(s) = self.peek_n(2)
                    && s == "\"\""
                {
                    // consume the two extra quotes
                    self.read();
                    self.read();
                    return Some(MapNode::BlockComment(self.parse_block_comment()));
                }
            }
            '.' => {
                // Check for spread: ...$ or ...@
                if let Some(s) = self.peek_n(2)
                    && s == ".."
                {
                    let save = self.save();
                    self.read(); // second .
                    self.read(); // third .
                    if let Some(next) = self.peek() {
                        if next == '$' {
                            self.read(); // $
                            let subst = self.parse_substitution(true);
                            if let Some(subst) = subst {
                                return Some(MapNode::Substitution(subst));
                            }
                            return None;
                        }
                        if next == '@' {
                            self.read(); // @
                            let imp = self.parse_import(true);
                            return Some(MapNode::Import(imp));
                        }
                    }
                    self.restore(save);
                }
            }
            _ => {}
        }

        // Unread r and try to parse a map key.
        self.unread(r);
        self.parse_map_key().map(|k| MapNode::Key(Box::new(k)))
    }

    // -----------------------------------------------------------------------
    // parse_comment
    // -----------------------------------------------------------------------

    fn parse_comment(&mut self) -> Comment {
        let start = self.pos.subtract('#');
        let mut value = String::new();
        self.parse_comment_line(&mut value);

        // Continue if next non-space is # on the very next line.
        loop {
            let save = self.save();
            let newlines = self.skip_whitespace();
            if self.eof() {
                self.restore(save);
                break;
            }
            let ch = self.peek();
            if ch != Some('#') || newlines >= 2 {
                self.restore(save);
                break;
            }
            self.read(); // consume #

            if newlines == 1 {
                value.push('\n');
            }
            self.parse_comment_line(&mut value);
        }

        Comment {
            range: Range::new(self.path.clone(), start, self.pos),
            value,
        }
    }

    fn parse_comment_line(&mut self, sb: &mut String) {
        let mut first_rune = true;
        loop {
            let Some(ch) = self.peek() else {
                return;
            };
            if ch == '\n' {
                return;
            }
            self.read(); // consume
            if first_rune {
                first_rune = false;
                if ch == ' ' {
                    continue;
                }
            }
            sb.push(ch);
        }
    }

    // -----------------------------------------------------------------------
    // parse_block_comment
    // -----------------------------------------------------------------------

    fn parse_block_comment(&mut self) -> BlockComment {
        let start = self.pos.subtract_string("\"\"\"");
        self.depth += 1;

        // Skip leading whitespace up to and including first newline.
        while let Some(ch) = self.peek() {
            if !ch.is_whitespace() {
                break;
            }
            self.read();
            if ch == '\n' {
                break;
            }
        }

        let mut sb = String::new();
        loop {
            let Some(ch) = self.read() else {
                let end = self.pos;
                self.errorf(
                    start,
                    end,
                    "block comments must be terminated with \"\"\"".into(),
                );
                self.depth -= 1;
                let value = trim_space_after_last_newline(&sb);
                let value = trim_common_indent(&value);
                return BlockComment {
                    range: Range::new(self.path.clone(), start, self.pos),
                    value,
                };
            };

            if ch != '"' {
                sb.push(ch);
                continue;
            }

            // Check for closing """
            if self.peek_n(2) == Some("\"\"".to_string()) {
                self.read();
                self.read();
                self.depth -= 1;
                let value = trim_space_after_last_newline(&sb);
                let value = trim_common_indent(&value);
                return BlockComment {
                    range: Range::new(self.path.clone(), start, self.pos),
                    value,
                };
            }

            sb.push('"');
        }
    }

    // -----------------------------------------------------------------------
    // parse_map_key
    // -----------------------------------------------------------------------

    fn parse_map_key(&mut self) -> Option<Key> {
        let start = self.pos;
        let mut mk = Key {
            range: Range::new(self.path.clone(), start, Position::default()),
            ampersand: false,
            not_ampersand: false,
            key: None,
            edges: Vec::new(),
            edge_index: None,
            edge_key: None,
            primary: None,
            value: None,
        };

        // Check for !& or &
        if let Some(ch) = self.peek() {
            if ch == '!' {
                let save = self.save();
                self.read();
                if self.peek() == Some('&') {
                    self.read();
                    mk.not_ampersand = true;
                } else {
                    self.restore(save);
                }
            } else if ch == '&' {
                self.read();
                mk.ampersand = true;
            }
        }

        // Check for edge group (
        if let Some(ch) = self.peek()
            && ch == '('
        {
            self.read();
            self.parse_edge_group(&mut mk);
            mk.range.end = self.pos;
            return if mk.key.is_none() && mk.edges.is_empty() {
                None
            } else {
                Some(mk)
            };
        }

        let k = self.parse_key_path();
        if let Some(k) = k {
            mk.key = Some(k);
        }

        // Peek for edge operators or colon
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            mk.range.end = self.pos;
            return if mk.key.is_none() && mk.edges.is_empty() {
                None
            } else {
                Some(mk)
            };
        }

        let ch = self.peek();
        match ch {
            Some('(') => {
                self.read();
                let src_key = mk.key.take();
                // Re-set start for edge group
                self.parse_edge_group(&mut mk);
                if let Some(sk) = src_key {
                    mk.key = Some(sk);
                }
                mk.range.end = self.pos;
                if mk.key.is_none() && mk.edges.is_empty() {
                    None
                } else {
                    Some(mk)
                }
            }
            Some('<') | Some('>') | Some('-') => {
                self.restore(save);
                let src = mk.key.take();
                self.parse_edges(&mut mk, src);
                self.parse_map_key_value(&mut mk);
                mk.range.end = self.pos;
                if mk.key.is_none() && mk.edges.is_empty() {
                    None
                } else {
                    Some(mk)
                }
            }
            _ => {
                self.restore(save);
                self.parse_map_key_value(&mut mk);
                mk.range.end = self.pos;
                if mk.key.is_none() && mk.edges.is_empty() {
                    None
                } else {
                    Some(mk)
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // parse_map_key_value
    // -----------------------------------------------------------------------

    fn parse_map_key_value(&mut self, mk: &mut Key) {
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            return;
        }

        let ch = self.peek();
        match ch {
            Some('{') => {
                self.restore(save);
                if mk.key.is_none() && mk.edges.is_empty() {
                    return;
                }
            }
            Some(':') => {
                self.read(); // consume ':'
                if mk.key.is_none() && mk.edges.is_empty() {
                    let end = self.pos;
                    self.errorf(mk.range.start, end, "map value without key".into());
                }
            }
            _ => {
                self.restore(save);
                return;
            }
        }

        let colon_pos = self.pos;
        mk.value = self.parse_value();
        if mk.value.is_none() {
            let end = self.pos;
            self.errorf(
                colon_pos.subtract(':'),
                end,
                "missing value after colon".into(),
            );
            return;
        }

        // If the value is a scalar, check if there's also a map following (primary + value).
        let is_scalar = mk.value.as_ref().is_some_and(|v| v.scalar_box().is_some());
        if is_scalar {
            let save2 = self.save();
            let newlines2 = self.skip_whitespace();
            if self.eof() || newlines2 > 0 {
                self.restore(save2);
                return;
            }
            if self.peek() == Some('{') {
                // The current value becomes the primary, and we parse the map as the value.
                let current = mk.value.take().unwrap();
                mk.primary = current.scalar_box();
                mk.value = self.parse_value();
            } else {
                self.restore(save2);
            }
        }
    }

    // -----------------------------------------------------------------------
    // parse_edge_group
    // -----------------------------------------------------------------------

    fn parse_edge_group(&mut self, mk: &mut Key) {
        let old_in_edge_group = self.in_edge_group;
        self.in_edge_group = true;

        let src = self.parse_key_path();
        self.parse_edges(mk, src);

        self.in_edge_group = old_in_edge_group;

        // Expect )
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            return;
        }
        if self.peek() != Some(')') {
            self.restore(save);
            let end = self.pos;
            self.errorf(
                mk.range.start,
                end,
                "edge groups must be terminated with )".into(),
            );
            return;
        }
        self.read(); // consume )

        // Optional edge index [n] or [*]
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            self.in_edge_group = false;
            self.parse_map_key_value(mk);
            return;
        }
        if self.peek() == Some('[') {
            self.read();
            mk.edge_index = self.parse_edge_index();
        } else {
            self.restore(save);
        }

        // Optional edge key .key
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            self.in_edge_group = false;
            self.parse_map_key_value(mk);
            return;
        }
        if self.peek() == Some('.') {
            self.read();
            mk.edge_key = self.parse_key_path();
        } else {
            self.restore(save);
        }

        self.in_edge_group = false;
        self.parse_map_key_value(mk);
    }

    // -----------------------------------------------------------------------
    // parse_edge_index
    // -----------------------------------------------------------------------

    fn parse_edge_index(&mut self) -> Option<EdgeIndex> {
        let start = self.pos.subtract('[');
        let mut ei = EdgeIndex {
            range: Range::new(self.path.clone(), start, Position::default()),
            int: None,
            glob: false,
        };

        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            return None;
        }

        let ch = self.peek()?;

        if ch.is_ascii_digit() {
            let mut num_str = String::new();
            // Read digits
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    num_str.push(c);
                    self.read();
                } else if c == ']' {
                    break;
                } else if c.is_whitespace() {
                    self.skip_whitespace();
                    break;
                } else {
                    let start_err = self.pos;
                    self.read();
                    let end_err = self.pos;
                    self.errorf(
                        start_err,
                        end_err,
                        "unexpected character in edge index".into(),
                    );
                }
            }
            if let Ok(i) = num_str.parse::<i64>() {
                ei.int = Some(i);
            }
        } else if ch == '*' {
            self.read();
            ei.glob = true;
        } else {
            let start_err = self.pos;
            self.read();
            let end_err = self.pos;
            self.errorf(
                start_err,
                end_err,
                "unexpected character in edge index".into(),
            );
        }

        // Expect ]
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 || self.peek() != Some(']') {
            self.restore(save);
            let end = self.pos;
            self.errorf(ei.range.start, end, "unterminated edge index".into());
            ei.range.end = self.pos;
            return Some(ei);
        }
        self.read(); // consume ]
        ei.range.end = self.pos;
        Some(ei)
    }

    // -----------------------------------------------------------------------
    // parse_edges
    // -----------------------------------------------------------------------

    fn parse_edges(&mut self, mk: &mut Key, src: Option<KeyPath>) {
        let mut current_src = src;
        loop {
            let mut e = Edge {
                range: Range::new(
                    self.path.clone(),
                    current_src.as_ref().map_or(self.pos, |s| s.range.start),
                    Position::default(),
                ),
                src: current_src.clone(),
                src_arrow: String::new(),
                dst: None,
                dst_arrow: String::new(),
            };

            let save = self.save();
            let newlines = self.skip_whitespace();
            if self.eof() || newlines > 0 {
                self.restore(save);
                return;
            }

            let ch = self.peek().unwrap();
            match ch {
                '<' | '*' => {
                    e.src_arrow = ch.to_string();
                    self.read();
                }
                '-' => {
                    // do not consume yet, parse_edge will handle
                }
                _ => {
                    self.restore(save);
                    return;
                }
            }

            if e.src.is_none() {
                let start_err = self.pos.subtract(ch);
                let end_err = self.pos;
                self.errorf(start_err, end_err, "connection missing source".into());
            }

            if ch == '<' || ch == '*' {
                // We already consumed the src arrow char. Now parse the rest of the edge (--, ->, etc.)
                if !self.parse_edge_body(&mut e) {
                    return;
                }
            } else {
                // ch == '-', parse_edge_body will consume it
                if !self.parse_edge_body(&mut e) {
                    return;
                }
            }

            let dst = self.parse_key_path();
            if dst.is_none() {
                let end = self.pos;
                self.errorf(e.range.start, end, "connection missing destination".into());
            } else {
                e.dst = dst.clone();
                if let Some(ref d) = e.dst {
                    e.range.end = d.range.end;
                }
            }
            current_src = dst;
            mk.edges.push(e);
        }
    }

    /// Parse the arrow body: dashes and optional dst arrow (> or *).
    /// Returns false if the edge is malformed/unterminated.
    fn parse_edge_body(&mut self, e: &mut Edge) -> bool {
        loop {
            let Some(ch) = self.read() else {
                let end = self.pos;
                self.errorf(e.range.start, end, "unterminated connection".into());
                return false;
            };

            match ch {
                '>' | '*' => {
                    e.dst_arrow = ch.to_string();
                    e.range.end = self.pos;
                    return true;
                }
                '\\' => {
                    // Line continuation inside edge
                    let save = self.save();
                    let newlines = self.skip_whitespace();
                    if self.eof() {
                        continue;
                    }
                    if newlines == 0 {
                        self.restore(save);
                        let end = self.pos;
                        self.errorf(
                            e.range.start,
                            end,
                            "only newline escapes are allowed in connections".into(),
                        );
                        return false;
                    }
                    // Consumed newline escape, continue parsing edge
                }
                '-' => {
                    // part of the arrow, continue
                }
                _ => {
                    self.unread(ch);
                    e.range.end = self.pos;
                    return true;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // parse_key_path
    // -----------------------------------------------------------------------

    fn parse_key_path(&mut self) -> Option<KeyPath> {
        let start = self.pos;
        let mut path: Vec<StringBox> = Vec::new();

        loop {
            let save = self.save();
            let newlines = self.skip_whitespace();
            if self.eof() || newlines > 0 {
                self.restore(save);
                break;
            }
            let ch = self.peek();
            if ch == Some('(') {
                self.restore(save);
                break;
            }
            if ch == Some('.') && !path.is_empty() {
                self.read(); // consume dot separator
                continue;
            } else if ch == Some('.') && path.is_empty() {
                // Dot at start; not a separator, will be part of unquoted string or error
                self.restore(save);
            } else {
                self.restore(save);
            }

            if let Some(sb) = self.parse_string_node(true) {
                path.push(sb);
            } else {
                break;
            }

            // Check for dot separator
            let save = self.save();
            let newlines = self.skip_whitespace();
            if self.eof() || newlines > 0 {
                self.restore(save);
                break;
            }
            if self.peek() != Some('.') {
                self.restore(save);
                break;
            }
            self.read(); // consume '.'
        }

        if path.is_empty() {
            return None;
        }

        // Validate key length
        let end = path.last().unwrap().get_range().end;
        for part in &path {
            if part.scalar_string().len() > 518 {
                self.errorf(
                    start,
                    end,
                    format!(
                        "key length {} exceeds maximum allowed length of 518",
                        part.scalar_string().len()
                    ),
                );
                break;
            }
        }

        Some(KeyPath {
            range: Range::new(self.path.clone(), start, end),
            path,
        })
    }

    // -----------------------------------------------------------------------
    // parse_string_node (returns StringBox)
    // -----------------------------------------------------------------------

    fn parse_string_node(&mut self, in_key: bool) -> Option<StringBox> {
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            return None;
        }

        let ch = self.peek()?;
        match ch {
            '"' => {
                self.read();
                Some(StringBox::DoubleQuoted(
                    self.parse_double_quoted_string(in_key),
                ))
            }
            '\'' => {
                self.read();
                Some(StringBox::SingleQuoted(self.parse_single_quoted_string()))
            }
            '|' => {
                self.read();
                Some(StringBox::Block(self.parse_block_string()))
            }
            _ => self.parse_unquoted_string(in_key).map(StringBox::Unquoted),
        }
    }

    // -----------------------------------------------------------------------
    // parse_unquoted_string
    // -----------------------------------------------------------------------

    fn parse_unquoted_string(&mut self, in_key: bool) -> Option<UnquotedString> {
        let start = self.pos;
        let mut sb = String::new();
        let mut rawb = String::new();
        let mut has_substitution = false;
        let mut value_parts: Vec<InterpolationBox> = Vec::new();
        let mut last_non_space = start;
        let mut pattern: Option<Vec<String>> = None;
        let mut last_pattern_index: usize = 0;

        while let Some(ch) = self.peek() {
            // If in edge group, ')' handling
            if self.in_edge_group && ch == ')' {
                break;
            }

            // Terminators for all contexts
            match ch {
                '\n' | ';' | '#' | '{' | '}' | '[' | ']' => break,
                _ => {}
            }

            if in_key {
                match ch {
                    ':' | '.' | '<' | '>' | '&' => break,
                    '-' => {
                        // Peek at next char: if it's -, > or *, this ends the key
                        self.read(); // consume '-'
                        let next = self.peek();
                        match next {
                            Some('-') | Some('>') | Some('*') => {
                                self.unread('-');
                                break;
                            }
                            Some('\n') | Some(';') | Some('#') | Some('{') | Some('}')
                            | Some('[') | Some(']') => {
                                sb.push('-');
                                rawb.push('-');
                                break;
                            }
                            None => {
                                sb.push('-');
                                rawb.push('-');
                                break;
                            }
                            Some(_) => {
                                // The '-' is part of the key (e.g. stroke-dash).
                                // We already consumed '-'. Just push it and let
                                // the outer loop process the next char normally.
                                sb.push('-');
                                rawb.push('-');
                                last_non_space = self.pos;
                                continue;
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Glob pattern tracking
            if ch == '*' {
                if pattern.is_none() {
                    pattern = Some(Vec::new());
                }
                if let Some(ref mut pat) = pattern {
                    if sb.is_empty() {
                        pat.push("*".to_string());
                    } else {
                        pat.push(sb[last_pattern_index..].to_string());
                        pat.push("*".to_string());
                    }
                    last_pattern_index = sb.len() + 1;
                }
            }

            self.read(); // consume ch

            if !ch.is_whitespace() {
                last_non_space = self.pos;
            }

            // Substitution handling (only in values, not keys)
            if !in_key && ch == '$' {
                let subst = self.parse_substitution(false);
                if let Some(subst) = subst {
                    has_substitution = true;
                    if !sb.is_empty() {
                        let sv = sb.clone();
                        let rv = rawb.clone();
                        value_parts.push(InterpolationBox {
                            string: Some(sv),
                            string_raw: Some(rv),
                            substitution: None,
                        });
                        sb.clear();
                        rawb.clear();
                    }
                    value_parts.push(InterpolationBox {
                        string: None,
                        string_raw: None,
                        substitution: Some(subst),
                    });
                    continue;
                }
                continue;
            }

            // Escape handling
            if ch == '\\' {
                let Some(ch2) = self.read() else {
                    let start_err = self.pos.subtract('\\');
                    let end_err = self.pos;
                    self.errorf(start_err, end_err, "unfinished escape sequence".into());
                    break;
                };
                if ch2 == '\n' {
                    // Line continuation
                    let save = self.save();
                    let newlines = self.skip_whitespace();
                    if self.eof() || newlines > 0 {
                        self.restore(save);
                        break;
                    }
                    continue;
                }
                sb.push(decode_escape(ch2));
                rawb.push('\\');
                rawb.push(ch2);
                continue;
            }

            sb.push(ch);
            rawb.push(ch);
        }

        // Trim trailing whitespace from the string value.
        let sv = sb.trim_end().to_string();
        let rv = rawb.trim_end().to_string();

        if sv.is_empty() && !has_substitution && value_parts.is_empty() {
            return None;
        }

        // Finalize pattern
        if let Some(ref mut pat) = pattern
            && last_pattern_index < sv.len()
        {
            pat.push(sv[last_pattern_index..].to_string());
        }

        if !sv.is_empty() {
            value_parts.push(InterpolationBox {
                string: Some(sv),
                string_raw: Some(rv),
                substitution: None,
            });
        }

        Some(UnquotedString {
            range: Range::new(self.path.clone(), start, last_non_space),
            value: value_parts,
            pattern,
        })
    }

    // -----------------------------------------------------------------------
    // parse_double_quoted_string
    // -----------------------------------------------------------------------

    fn parse_double_quoted_string(&mut self, in_key: bool) -> DoubleQuotedString {
        let start = self.pos.subtract('"');
        let mut value_parts: Vec<InterpolationBox> = Vec::new();
        let mut sb = String::new();
        let mut rawb = String::new();

        loop {
            let Some(ch) = self.peek() else {
                // EOF
                let end = self.pos;
                self.errorf(
                    start,
                    end,
                    "double quoted strings must be terminated with \"".into(),
                );
                if !sb.is_empty() {
                    let sv = sb.clone();
                    let rv = rawb.clone();
                    value_parts.push(InterpolationBox {
                        string: Some(sv),
                        string_raw: Some(rv),
                        substitution: None,
                    });
                }
                return DoubleQuotedString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    value: value_parts,
                };
            };

            if ch == '\n' {
                let end = self.pos;
                self.errorf(
                    start,
                    end,
                    "double quoted strings must be terminated with \"".into(),
                );
                if !sb.is_empty() {
                    let sv = sb.clone();
                    let rv = rawb.clone();
                    value_parts.push(InterpolationBox {
                        string: Some(sv),
                        string_raw: Some(rv),
                        substitution: None,
                    });
                }
                return DoubleQuotedString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    value: value_parts,
                };
            }

            self.read(); // consume ch

            // Substitution in value context
            if !in_key && ch == '$' {
                let subst = self.parse_substitution(false);
                if let Some(subst) = subst {
                    if !sb.is_empty() {
                        let sv = sb.clone();
                        let rv = rawb.clone();
                        value_parts.push(InterpolationBox {
                            string: Some(sv),
                            string_raw: Some(rv),
                            substitution: None,
                        });
                        sb.clear();
                        rawb.clear();
                    }
                    value_parts.push(InterpolationBox {
                        string: None,
                        string_raw: None,
                        substitution: Some(subst),
                    });
                    continue;
                }
            }

            if ch == '"' {
                if !sb.is_empty() {
                    let sv = sb.clone();
                    let rv = rawb.clone();
                    value_parts.push(InterpolationBox {
                        string: Some(sv),
                        string_raw: Some(rv),
                        substitution: None,
                    });
                }
                return DoubleQuotedString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    value: value_parts,
                };
            }

            if ch != '\\' {
                sb.push(ch);
                rawb.push(ch);
                continue;
            }

            // Escape sequence
            let Some(ch2) = self.read() else {
                let s1 = self.pos.subtract('\\');
                let e1 = self.pos;
                self.errorf(s1, e1, "unfinished escape sequence".into());
                self.errorf(
                    start,
                    self.pos,
                    "double quoted strings must be terminated with \"".into(),
                );
                if !sb.is_empty() {
                    let sv = sb.clone();
                    let rv = rawb.clone();
                    value_parts.push(InterpolationBox {
                        string: Some(sv),
                        string_raw: Some(rv),
                        substitution: None,
                    });
                }
                return DoubleQuotedString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    value: value_parts,
                };
            };

            if ch2 == '\n' {
                // Newline escape (line continuation inside double-quoted string)
                continue;
            }
            sb.push(decode_escape(ch2));
            rawb.push('\\');
            rawb.push(ch2);
        }
    }

    // -----------------------------------------------------------------------
    // parse_single_quoted_string
    // -----------------------------------------------------------------------

    fn parse_single_quoted_string(&mut self) -> SingleQuotedString {
        let start = self.pos.subtract('\'');
        let mut sb = String::new();

        loop {
            let Some(ch) = self.peek() else {
                let end = self.pos;
                self.errorf(
                    start,
                    end,
                    "single quoted strings must be terminated with '".into(),
                );
                return SingleQuotedString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    raw: String::new(),
                    value: sb,
                };
            };

            if ch == '\n' {
                let end = self.pos;
                self.errorf(
                    start,
                    end,
                    "single quoted strings must be terminated with '".into(),
                );
                return SingleQuotedString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    raw: String::new(),
                    value: sb,
                };
            }

            self.read(); // consume ch

            if ch == '\'' {
                // Check for escaped quote ''
                if self.peek() == Some('\'') {
                    self.read();
                    sb.push('\'');
                    continue;
                }
                return SingleQuotedString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    raw: String::new(),
                    value: sb,
                };
            }

            if ch == '\\' {
                if self.peek() == Some('\n') {
                    self.read(); // consume the newline (line continuation)
                    continue;
                }
                sb.push(ch);
                continue;
            }

            sb.push(ch);
        }
    }

    // -----------------------------------------------------------------------
    // parse_block_string
    // -----------------------------------------------------------------------

    fn parse_block_string(&mut self) -> BlockString {
        let start = self.pos.subtract('|');
        self.depth += 1;

        let mut quote = String::new();
        let mut tag = String::new();

        // Read additional quote chars (e.g. |||)
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || ch.is_alphanumeric() || ch == '_' {
                break;
            }
            self.read();
            quote.push(ch);
        }

        // Read tag
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                break;
            }
            self.read();
            tag.push(ch);
        }

        if tag.is_empty() {
            tag = "md".to_string();
        }

        // Skip whitespace up to first newline (or first non-whitespace)
        let mut sb = String::new();
        loop {
            let Some(ch) = self.peek() else {
                let end = self.pos;
                self.errorf(
                    start,
                    end,
                    format!("block string must be terminated with {}|", quote),
                );
                self.depth -= 1;
                let value = trim_space_after_last_newline(&sb);
                let value = trim_common_indent(&value);
                return BlockString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    quote,
                    tag,
                    value,
                };
            };
            if !ch.is_whitespace() {
                // Non-whitespace on the first line gets implicit indent.
                let indent = " ".repeat(self.depth * 2);
                sb.push_str(&indent);
                break;
            }
            self.read();
            if ch == '\n' {
                break;
            }
        }

        // Build the end marker
        let end_marker = if quote.is_empty() {
            "|".to_string()
        } else {
            format!("{}|", quote)
        };

        loop {
            let Some(ch) = self.read() else {
                let end = self.pos;
                self.errorf(
                    start,
                    end,
                    format!("block string must be terminated with {}|", quote),
                );
                self.depth -= 1;
                let value = trim_space_after_last_newline(&sb);
                let value = trim_common_indent(&value);
                return BlockString {
                    range: Range::new(self.path.clone(), start, self.pos),
                    quote,
                    tag,
                    value,
                };
            };

            // Check if ch starts the end marker
            if !end_marker.is_empty() {
                let end_chars: Vec<char> = end_marker.chars().collect();
                if ch == end_chars[0] {
                    if end_chars.len() == 1 {
                        // Matched full end marker
                        self.depth -= 1;
                        let value = trim_space_after_last_newline(&sb);
                        let value = trim_common_indent(&value);
                        return BlockString {
                            range: Range::new(self.path.clone(), start, self.pos),
                            quote,
                            tag,
                            value,
                        };
                    }
                    // Check if rest of end marker follows
                    let rest: String = end_chars[1..].iter().collect();
                    if let Some(peeked) = self.peek_n(rest.len())
                        && peeked == rest
                    {
                        // Consume rest
                        for _ in 0..rest.len() {
                            self.read();
                        }
                        self.depth -= 1;
                        let value = trim_space_after_last_newline(&sb);
                        let value = trim_common_indent(&value);
                        return BlockString {
                            range: Range::new(self.path.clone(), start, self.pos),
                            quote,
                            tag,
                            value,
                        };
                    }
                    sb.push(ch);
                    continue;
                }
            }

            sb.push(ch);
        }
    }

    // -----------------------------------------------------------------------
    // parse_array
    // -----------------------------------------------------------------------

    fn parse_array(&mut self) -> Array {
        let start = self.pos.subtract('[');
        self.depth += 1;

        let mut nodes: Vec<ArrayNode> = Vec::new();

        loop {
            let Some(r) = self.read_not_space() else {
                let end = self.pos;
                self.errorf(start, end, "arrays must be terminated with ]".into());
                self.depth -= 1;
                return Array {
                    range: Range::new(self.path.clone(), start, self.pos),
                    nodes,
                };
            };

            match r {
                ';' => continue,
                ']' => {
                    self.depth -= 1;
                    return Array {
                        range: Range::new(self.path.clone(), start, self.pos),
                        nodes,
                    };
                }
                _ => {}
            }

            if let Some(n) = self.parse_array_node(r) {
                nodes.push(n);
            }

            // Consume unexpected trailing text
            let after = self.pos;
            loop {
                let (ch, newlines) = self.peek_not_space();
                match ch {
                    None => break,
                    Some(c) if newlines != 0 || c == ';' || c == ']' || c == '#' => break,
                    Some(_) => {
                        self.skip_whitespace();
                        let _ = self.read();
                    }
                }
            }
            if after != self.pos {
                let end = self.pos;
                self.errorf(after, end, "unexpected text after value".into());
            }
        }
    }

    fn parse_array_node(&mut self, r: char) -> Option<ArrayNode> {
        match r {
            '#' => {
                return Some(ArrayNode::Comment(self.parse_comment()));
            }
            '"' => {
                if let Some(s) = self.peek_n(2)
                    && s == "\"\""
                {
                    self.read();
                    self.read();
                    return Some(ArrayNode::BlockComment(self.parse_block_comment()));
                }
            }
            '.' => {
                if let Some(s) = self.peek_n(2)
                    && s == ".."
                {
                    let save = self.save();
                    self.read();
                    self.read();
                    if let Some(next) = self.peek() {
                        if next == '$' {
                            self.read();
                            if let Some(subst) = self.parse_substitution(true) {
                                return Some(ArrayNode::Substitution(subst));
                            }
                            return None;
                        }
                        if next == '@' {
                            self.read();
                            return Some(ArrayNode::Import(self.parse_import(true)));
                        }
                    }
                    self.restore(save);
                }
            }
            _ => {}
        }

        self.unread(r);
        let vbox = self.parse_value();
        vbox.map(|v| match v {
            ValueBox::Null(n) => ArrayNode::Null(n),
            ValueBox::Boolean(b) => ArrayNode::Boolean(b),
            ValueBox::Number(n) => ArrayNode::Number(n),
            ValueBox::UnquotedString(s) => ArrayNode::UnquotedString(s),
            ValueBox::DoubleQuotedString(s) => ArrayNode::DoubleQuotedString(s),
            ValueBox::SingleQuotedString(s) => ArrayNode::SingleQuotedString(s),
            ValueBox::BlockString(s) => ArrayNode::BlockString(s),
            ValueBox::Array(a) => ArrayNode::Array(a),
            ValueBox::Map(m) => ArrayNode::Map(m),
            ValueBox::Import(i) => ArrayNode::Import(i),
            ValueBox::Suspension(_) => ArrayNode::Null(Null {
                range: Range::default(),
            }),
        })
    }

    // -----------------------------------------------------------------------
    // parse_value
    // -----------------------------------------------------------------------

    fn parse_value(&mut self) -> Option<ValueBox> {
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            return None;
        }

        let ch = self.peek()?;
        match ch {
            '[' => {
                self.read();
                return Some(ValueBox::Array(Box::new(self.parse_array())));
            }
            '{' => {
                self.read();
                return Some(ValueBox::Map(Box::new(self.parse_map(false))));
            }
            '@' => {
                self.read();
                return Some(ValueBox::Import(self.parse_import(false)));
            }
            _ => {}
        }

        let sb = self.parse_string_node(false)?;

        match sb {
            StringBox::DoubleQuoted(s) => Some(ValueBox::DoubleQuotedString(s)),
            StringBox::SingleQuoted(s) => Some(ValueBox::SingleQuotedString(s)),
            StringBox::Block(s) => Some(ValueBox::BlockString(s)),
            StringBox::Unquoted(s) => {
                let scalar = s.scalar_string().to_string();
                let lower = scalar.to_lowercase();

                if lower == "null" {
                    return Some(ValueBox::Null(Null { range: s.range }));
                }
                if lower == "suspend" {
                    return Some(ValueBox::Suspension(Suspension {
                        range: s.range,
                        value: true,
                    }));
                }
                if lower == "unsuspend" {
                    return Some(ValueBox::Suspension(Suspension {
                        range: s.range,
                        value: false,
                    }));
                }
                if lower == "true" {
                    return Some(ValueBox::Boolean(Boolean {
                        range: s.range,
                        value: true,
                    }));
                }
                if lower == "false" {
                    return Some(ValueBox::Boolean(Boolean {
                        range: s.range,
                        value: false,
                    }));
                }

                // Try parsing as a number
                if let Ok(v) = scalar.parse::<f64>() {
                    // Only treat as number if it looks like one (digits, optional dots, optional
                    // sign, etc.)
                    if is_numeric_string(&scalar) {
                        return Some(ValueBox::Number(Number {
                            range: s.range,
                            raw: scalar,
                            value: v,
                        }));
                    }
                }

                Some(ValueBox::UnquotedString(s))
            }
        }
    }

    // -----------------------------------------------------------------------
    // parse_substitution
    // -----------------------------------------------------------------------

    fn parse_substitution(&mut self, spread: bool) -> Option<Substitution> {
        let dollar_start = self.pos.subtract_string("$");
        let start = if spread {
            dollar_start.subtract_string("...")
        } else {
            dollar_start
        };

        // Expect {
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() || newlines > 0 {
            self.restore(save);
            return None;
        }
        if self.peek() != Some('{') {
            self.restore(save);
            return None;
        }
        self.read(); // consume {

        let k = self.parse_key_path();
        let path = k.map_or_else(Vec::new, |k| k.path);

        // Expect }
        let save = self.save();
        let newlines = self.skip_whitespace();
        if self.eof() {
            let end = self.pos;
            self.errorf(start, end, "substitutions must be terminated by }".into());
            return Some(Substitution {
                range: Range::new(self.path.clone(), start, self.pos),
                spread,
                path,
            });
        }
        if newlines > 0 || self.peek() != Some('}') {
            self.restore(save);
            let end = self.pos;
            self.errorf(start, end, "substitutions must be terminated by }".into());
            return Some(Substitution {
                range: Range::new(self.path.clone(), start, self.pos),
                spread,
                path,
            });
        }
        self.read(); // consume }

        Some(Substitution {
            range: Range::new(self.path.clone(), start, self.pos),
            spread,
            path,
        })
    }

    // -----------------------------------------------------------------------
    // parse_import
    // -----------------------------------------------------------------------

    fn parse_import(&mut self, spread: bool) -> Import {
        let at_start = self.pos.subtract_string("@");
        let start = if spread {
            at_start.subtract_string("...")
        } else {
            at_start
        };

        let mut pre = String::new();
        while let Some(ch) = self.peek() {
            if ch != '.' && ch != '/' {
                break;
            }
            self.read();
            pre.push(ch);
        }

        let k = self.parse_key_path();
        let path = k.map_or_else(Vec::new, |k| k.path);

        Import {
            range: Range::new(self.path.clone(), start, self.pos),
            spread,
            pre,
            path,
        }
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

fn decode_escape(ch: char) -> char {
    match ch {
        'a' => '\u{07}',
        'b' => '\u{08}',
        'f' => '\u{0C}',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'v' => '\u{0B}',
        '\\' => '\\',
        '"' => '"',
        other => other,
    }
}

/// Check if a string looks numeric (integer or decimal, with optional sign).
fn is_numeric_string(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let s = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    if s.is_empty() {
        return false;
    }
    let mut has_dot = false;
    let mut has_slash = false;
    for ch in s.chars() {
        if ch == '.' && !has_dot {
            has_dot = true;
        } else if ch == '/' && !has_slash {
            // big.Rat format: 1/3
            has_slash = true;
        } else if !ch.is_ascii_digit() {
            return false;
        }
    }
    true
}

fn trim_space_after_last_newline(s: &str) -> String {
    if let Some(last_nl) = s.rfind('\n') {
        let last_line = &s[last_nl + 1..];
        let trimmed = last_line.trim_end();
        if trimmed.is_empty() {
            s[..last_nl].to_string()
        } else {
            format!("{}{}", &s[..last_nl + 1], trimmed)
        }
    } else {
        s.trim_end().to_string()
    }
}

/// Port of Go's `d2parser.splitLeadingIndent`. Walks whitespace at the
/// start of `s`, treating each `'\t'` as two space columns, until either
/// the first non-whitespace rune or `max_spaces` worth of columns have
/// been consumed. Returns the indent (measured in space columns) and the
/// byte offset at which the trailing text begins.
fn split_leading_indent(s: &str, max_spaces: Option<usize>) -> (usize, usize) {
    let mut indent_cols: usize = 0;
    let mut byte_off: usize = 0;
    for (idx, ch) in s.char_indices() {
        if !ch.is_whitespace() {
            byte_off = idx;
            return (indent_cols, byte_off);
        }
        byte_off = idx + ch.len_utf8();
        if ch == '\t' {
            indent_cols += 2;
        } else {
            indent_cols += 1;
        }
        if let Some(ms) = max_spaces
            && indent_cols == ms
        {
            return (indent_cols, byte_off);
        }
    }
    (indent_cols, byte_off)
}

fn trim_common_indent(s: &str) -> String {
    // Mirror Go `d2parser.trimCommonIndent`: find the minimum indent
    // across all non-empty, non-whitespace-only lines (measured in
    // space columns with tabs counting as 2), then strip that many
    // columns from every line. Critical for byte-identical markdown
    // block content — e.g. a `|md` block that mixes one tab-indented
    // line with two-space-indented lines must have the leading space
    // removed from the 2-space lines, not just one byte.
    let lines: Vec<&str> = s.split('\n').collect();
    let mut common_indent: Option<usize> = None;

    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let (indent_cols, indent_bytes) = split_leading_indent(line, None);
        if line[indent_bytes..].is_empty() {
            // Whitespace-only line — Go skips these.
            continue;
        }
        if indent_cols == 0 {
            // Go's `lineIndent == ""` shortcut: no common indent, bail.
            return s.to_string();
        }
        common_indent = Some(match common_indent {
            Some(ci) => ci.min(indent_cols),
            None => indent_cols,
        });
    }

    let ci = match common_indent {
        Some(0) | None => return s.to_string(),
        Some(ci) => ci,
    };

    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                return String::new();
            }
            let (_, byte_off) = split_leading_indent(line, Some(ci));
            line[byte_off..].to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse and return (map, errors).
    fn p(input: &str) -> (Map, Vec<Error>) {
        let (m, err) = parse("", input);
        let errs = err.map_or_else(Vec::new, |e| e.errors);
        (m, errs)
    }

    #[test]
    fn test_empty() {
        let (m, errs) = p("");
        assert!(errs.is_empty());
        assert!(m.nodes.is_empty());
    }

    #[test]
    fn test_semicolons() {
        let (m, errs) = p(";;;;;");
        assert!(errs.is_empty());
        assert!(m.nodes.is_empty());
    }

    #[test]
    fn test_single_key() {
        let (m, errs) = p("x");
        assert!(errs.is_empty());
        assert_eq!(m.nodes.len(), 1);
        let key = m.nodes[0].as_key().unwrap();
        assert!(key.edges.is_empty());
        let kp = key.key.as_ref().unwrap();
        assert_eq!(kp.path.len(), 1);
        assert_eq!(kp.path[0].scalar_string(), "x");
    }

    #[test]
    fn test_edge() {
        let (m, errs) = p("x -> y");
        assert!(errs.is_empty());
        assert_eq!(m.nodes.len(), 1);
        let key = m.nodes[0].as_key().unwrap();
        assert_eq!(key.edges.len(), 1);
        let edge = &key.edges[0];
        assert_eq!(edge.src.as_ref().unwrap().path[0].scalar_string(), "x");
        assert_eq!(edge.dst.as_ref().unwrap().path[0].scalar_string(), "y");
        assert_eq!(edge.src_arrow, "");
        assert_eq!(edge.dst_arrow, ">");
    }

    #[test]
    fn test_multiple_edges() {
        let (m, errs) = p("x -> y -> z");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        assert_eq!(key.edges.len(), 2);
        assert_eq!(
            key.edges[0].src.as_ref().unwrap().path[0].scalar_string(),
            "x"
        );
        assert_eq!(
            key.edges[0].dst.as_ref().unwrap().path[0].scalar_string(),
            "y"
        );
        assert_eq!(
            key.edges[1].src.as_ref().unwrap().path[0].scalar_string(),
            "y"
        );
        assert_eq!(
            key.edges[1].dst.as_ref().unwrap().path[0].scalar_string(),
            "z"
        );
    }

    #[test]
    fn test_key_with_label() {
        let (m, errs) = p("x: hello world");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        assert_eq!(key.key.as_ref().unwrap().path[0].scalar_string(), "x");
        match key.value.as_ref().unwrap() {
            ValueBox::UnquotedString(s) => {
                assert_eq!(s.scalar_string(), "hello world");
            }
            other => panic!("expected UnquotedString, got {:?}", other),
        }
    }

    #[test]
    fn test_key_with_map() {
        let (m, errs) = p("x: {\n  y\n}");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        assert_eq!(key.key.as_ref().unwrap().path[0].scalar_string(), "x");
        match key.value.as_ref().unwrap() {
            ValueBox::Map(inner) => {
                assert_eq!(inner.nodes.len(), 1);
            }
            other => panic!("expected Map, got {:?}", other),
        }
    }

    #[test]
    fn test_dotted_key() {
        let (m, errs) = p("a.b.c");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        let kp = key.key.as_ref().unwrap();
        assert_eq!(kp.path.len(), 3);
        assert_eq!(kp.path[0].scalar_string(), "a");
        assert_eq!(kp.path[1].scalar_string(), "b");
        assert_eq!(kp.path[2].scalar_string(), "c");
    }

    #[test]
    fn test_style_keyword() {
        let (m, errs) = p("x.style.fill: red");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        let kp = key.key.as_ref().unwrap();
        assert_eq!(kp.path.len(), 3);
        assert_eq!(kp.path[0].scalar_string(), "x");
        assert_eq!(kp.path[1].scalar_string(), "style");
        assert_eq!(kp.path[2].scalar_string(), "fill");
        match key.value.as_ref().unwrap() {
            ValueBox::UnquotedString(s) => {
                assert_eq!(s.scalar_string(), "red");
            }
            other => panic!("expected UnquotedString, got {:?}", other),
        }
    }

    #[test]
    fn test_boolean_value() {
        let (m, errs) = p("x: true");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        match key.value.as_ref().unwrap() {
            ValueBox::Boolean(b) => assert!(b.value),
            other => panic!("expected Boolean, got {:?}", other),
        }
    }

    #[test]
    fn test_number_value() {
        let (m, errs) = p("x: 42");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        match key.value.as_ref().unwrap() {
            ValueBox::Number(n) => {
                assert_eq!(n.raw, "42");
                assert!((n.value - 42.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Number, got {:?}", other),
        }
    }

    #[test]
    fn test_null_value() {
        let (m, errs) = p("x: null");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        assert!(matches!(key.value.as_ref().unwrap(), ValueBox::Null(_)));
    }

    #[test]
    fn test_comment() {
        let (m, errs) = p("# hello");
        assert!(errs.is_empty());
        assert_eq!(m.nodes.len(), 1);
        match &m.nodes[0] {
            MapNode::Comment(c) => assert_eq!(c.value, "hello"),
            other => panic!("expected Comment, got {:?}", other),
        }
    }

    #[test]
    fn test_double_quoted_string() {
        let (m, errs) = p(r#"x: "hello world""#);
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        match key.value.as_ref().unwrap() {
            ValueBox::DoubleQuotedString(s) => {
                assert_eq!(s.scalar_string(), "hello world");
            }
            other => panic!("expected DoubleQuotedString, got {:?}", other),
        }
    }

    #[test]
    fn test_single_quoted_string() {
        let (m, errs) = p("x: 'hello world'");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        match key.value.as_ref().unwrap() {
            ValueBox::SingleQuotedString(s) => {
                assert_eq!(s.scalar_string(), "hello world");
            }
            other => panic!("expected SingleQuotedString, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_with_label() {
        let (m, errs) = p("x -> y: hello");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        assert_eq!(key.edges.len(), 1);
        match key.value.as_ref().unwrap() {
            ValueBox::UnquotedString(s) => {
                assert_eq!(s.scalar_string(), "hello");
            }
            other => panic!("expected UnquotedString, got {:?}", other),
        }
    }

    #[test]
    fn test_reverse_edge() {
        let (m, errs) = p("x <- y");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        assert_eq!(key.edges.len(), 1);
        let edge = &key.edges[0];
        assert_eq!(edge.src_arrow, "<");
        assert_eq!(edge.dst_arrow, "");
        assert_eq!(edge.src.as_ref().unwrap().path[0].scalar_string(), "x");
        assert_eq!(edge.dst.as_ref().unwrap().path[0].scalar_string(), "y");
    }

    #[test]
    fn test_bidirectional_edge() {
        let (m, errs) = p("x <-> y");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        assert_eq!(key.edges.len(), 1);
        let edge = &key.edges[0];
        assert_eq!(edge.src_arrow, "<");
        assert_eq!(edge.dst_arrow, ">");
    }

    #[test]
    fn test_multiple_keys() {
        let (m, errs) = p("x\ny\nz");
        assert!(errs.is_empty());
        assert_eq!(m.nodes.len(), 3);
    }

    #[test]
    fn test_nested_map() {
        let (m, errs) = p("a: {\n  b: {\n    c\n  }\n}");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        let inner = key.value.as_ref().unwrap().as_map().unwrap();
        let inner_key = inner.nodes[0].as_key().unwrap();
        let inner2 = inner_key.value.as_ref().unwrap().as_map().unwrap();
        assert_eq!(inner2.nodes.len(), 1);
    }

    #[test]
    fn test_array_value() {
        let (m, errs) = p("x: [1; 2; 3]");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        match key.value.as_ref().unwrap() {
            ValueBox::Array(arr) => {
                assert_eq!(arr.nodes.len(), 3);
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[test]
    fn test_sql_table_shape() {
        let input = r#"users: {
  shape: sql_table
  id: int
  name: varchar
}"#;
        let (m, errs) = p(input);
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        assert_eq!(key.key.as_ref().unwrap().path[0].scalar_string(), "users");
        let inner = key.value.as_ref().unwrap().as_map().unwrap();
        // shape, id, name => 3 nodes
        assert_eq!(inner.nodes.len(), 3);
    }

    #[test]
    fn test_escape_in_string() {
        let (m, errs) = p(r#"x: "hello\nworld""#);
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        match key.value.as_ref().unwrap() {
            ValueBox::DoubleQuotedString(s) => {
                assert_eq!(s.scalar_string(), "hello\nworld");
            }
            other => panic!("expected DoubleQuotedString, got {:?}", other),
        }
    }

    #[test]
    fn test_substitution_in_value() {
        let (m, errs) = p("x: ${vars.color}");
        assert!(errs.is_empty());
        let key = m.nodes[0].as_key().unwrap();
        match key.value.as_ref().unwrap() {
            ValueBox::UnquotedString(s) => {
                assert!(!s.value.is_empty());
                // Should have a substitution part
                let has_subst = s.value.iter().any(|ib| ib.substitution.is_some());
                assert!(
                    has_subst,
                    "expected substitution in value, got {:?}",
                    s.value
                );
            }
            other => panic!("expected UnquotedString with substitution, got {:?}", other),
        }
    }

    #[test]
    fn test_missing_map_value() {
        let (m, errs) = p("x:");
        // Should have error about missing value
        assert!(!errs.is_empty());
        assert_eq!(m.nodes.len(), 1);
    }

    #[test]
    fn test_unterminated_map() {
        let (m, errs) = p("x: {");
        assert!(!errs.is_empty());
        assert_eq!(m.nodes.len(), 1);
    }

    #[test]
    fn test_block_string() {
        let input = "x: |md\nhello\nworld\n|";
        let (m, errs) = p(input);
        assert!(errs.is_empty(), "errors: {:?}", errs);
        let key = m.nodes[0].as_key().unwrap();
        match key.value.as_ref().unwrap() {
            ValueBox::BlockString(bs) => {
                assert_eq!(bs.tag, "md");
                assert!(bs.value.contains("hello"));
                assert!(bs.value.contains("world"));
            }
            other => panic!("expected BlockString, got {:?}", other),
        }
    }

    #[test]
    fn test_keyword_tables() {
        assert!(BOARD_KEYWORDS.contains("layers"));
        assert!(BOARD_KEYWORDS.contains("scenarios"));
        assert!(BOARD_KEYWORDS.contains("steps"));
        assert!(STYLE_KEYWORDS.contains("fill"));
        assert!(STYLE_KEYWORDS.contains("stroke"));
        assert!(STYLE_KEYWORDS.contains("opacity"));
        assert!(RESERVED_KEYWORDS.contains("label"));
        assert!(RESERVED_KEYWORDS.contains("shape"));
        assert!(RESERVED_KEYWORDS.contains("fill"));
    }
}
