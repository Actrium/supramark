//! Regex-based lexer engine matching chroma's state machine model.

use fancy_regex::Regex;

/// Token types matching chroma's token type hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    // Top-level categories
    Text,
    Keyword,
    KeywordConstant,
    KeywordDeclaration,
    KeywordNamespace,
    KeywordType,
    Name,
    NameBuiltin,
    NameFunction,
    NameOther,
    NameVariable,
    Literal,
    LiteralNumber,
    LiteralNumberFloat,
    LiteralNumberHex,
    LiteralNumberInteger,
    LiteralNumberOct,
    LiteralNumberBin,
    LiteralString,
    LiteralStringChar,
    LiteralStringDouble,
    LiteralStringSingle,
    LiteralStringBacktick,
    LiteralStringEscape,
    LiteralStringInterpol,
    LiteralStringAffix,
    Operator,
    Punctuation,
    Comment,
    CommentSingle,
    CommentMultiline,
    CommentPreproc,
    Other,
}

impl TokenType {
    /// Get the sub-category (parent type) for hierarchical style lookup.
    pub fn sub_category(self) -> TokenType {
        match self {
            TokenType::KeywordConstant
            | TokenType::KeywordDeclaration
            | TokenType::KeywordNamespace
            | TokenType::KeywordType => TokenType::Keyword,

            TokenType::NameBuiltin
            | TokenType::NameFunction
            | TokenType::NameOther
            | TokenType::NameVariable => TokenType::Name,

            TokenType::LiteralNumber
            | TokenType::LiteralNumberFloat
            | TokenType::LiteralNumberHex
            | TokenType::LiteralNumberInteger
            | TokenType::LiteralNumberOct
            | TokenType::LiteralNumberBin => TokenType::Literal,

            TokenType::LiteralString
            | TokenType::LiteralStringChar
            | TokenType::LiteralStringDouble
            | TokenType::LiteralStringSingle
            | TokenType::LiteralStringBacktick
            | TokenType::LiteralStringEscape
            | TokenType::LiteralStringInterpol
            | TokenType::LiteralStringAffix => TokenType::Literal,

            TokenType::CommentSingle | TokenType::CommentMultiline | TokenType::CommentPreproc => {
                TokenType::Comment
            }

            _ => self,
        }
    }

    /// Get the top-level category for hierarchical style lookup.
    pub fn category(self) -> TokenType {
        match self {
            TokenType::Keyword
            | TokenType::KeywordConstant
            | TokenType::KeywordDeclaration
            | TokenType::KeywordNamespace
            | TokenType::KeywordType => TokenType::Keyword,

            TokenType::Name
            | TokenType::NameBuiltin
            | TokenType::NameFunction
            | TokenType::NameOther
            | TokenType::NameVariable => TokenType::Name,

            TokenType::Literal
            | TokenType::LiteralNumber
            | TokenType::LiteralNumberFloat
            | TokenType::LiteralNumberHex
            | TokenType::LiteralNumberInteger
            | TokenType::LiteralNumberOct
            | TokenType::LiteralNumberBin
            | TokenType::LiteralString
            | TokenType::LiteralStringChar
            | TokenType::LiteralStringDouble
            | TokenType::LiteralStringSingle
            | TokenType::LiteralStringBacktick
            | TokenType::LiteralStringEscape
            | TokenType::LiteralStringInterpol
            | TokenType::LiteralStringAffix => TokenType::Literal,

            TokenType::Comment
            | TokenType::CommentSingle
            | TokenType::CommentMultiline
            | TokenType::CommentPreproc => TokenType::Comment,

            _ => self,
        }
    }
}

/// A single token produced by the lexer.
#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub value: String,
}

/// Action after a rule matches.
#[derive(Debug, Clone)]
pub enum Action {
    /// No state change.
    None,
    /// Push a new state onto the stack.
    Push(String),
    /// Pop the top state from the stack.
    Pop,
}

/// A single rule in the lexer.
pub enum Rule {
    /// Simple rule: regex pattern -> single token type + action.
    Simple {
        pattern: Regex,
        token_type: TokenType,
        action: Action,
    },
    /// ByGroups rule: regex with capture groups -> token per group.
    ByGroups {
        pattern: Regex,
        group_types: Vec<TokenType>,
        action: Action,
    },
    /// Include rules from another state.
    Include(String),
}

/// State: a named list of rules.
pub struct State {
    pub name: String,
    pub rules: Vec<Rule>,
}

/// The lexer trait.
pub trait Lexer {
    fn tokenize(&self, source: &str) -> Vec<Token>;
}

/// A generic state-machine lexer.
pub struct StateMachineLexer {
    pub states: Vec<State>,
    pub ensure_nl: bool,
}

impl StateMachineLexer {
    /// Find a state by name.
    fn find_state(&self, name: &str) -> Option<usize> {
        self.states.iter().position(|s| s.name == name)
    }

    /// Collect rule indices from a state, expanding Includes.
    fn collect_rules(&self, state_idx: usize) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        for (rule_idx, rule) in self.states[state_idx].rules.iter().enumerate() {
            if let Rule::Include(state_name) = rule {
                if let Some(included_idx) = self.find_state(state_name) {
                    result.extend(self.collect_rules(included_idx));
                }
            } else {
                result.push((state_idx, rule_idx));
            }
        }
        result
    }
}

impl Lexer for StateMachineLexer {
    fn tokenize(&self, source: &str) -> Vec<Token> {
        let mut input = source.to_string();
        let mut newline_added = false;
        if self.ensure_nl && !input.ends_with('\n') {
            input.push('\n');
            newline_added = true;
        }
        // When EnsureNL adds a newline, the iterator stops one char before the
        // end so the added newline is never tokenized. This matches chroma's
        // LexerState.Iterator which uses `end := len(l.Text); if l.newlineAdded { end-- }`.
        let end = if newline_added {
            input.len() - 1
        } else {
            input.len()
        };

        let mut tokens = Vec::new();
        let mut pos = 0;
        let mut state_stack: Vec<usize> = vec![self.find_state("root").unwrap_or(0)];

        while pos < end {
            let current_state = *state_stack.last().unwrap();
            let rules = self.collect_rules(current_state);

            let mut matched = false;
            for &(state_idx, rule_idx) in &rules {
                let rule = &self.states[state_idx].rules[rule_idx];
                match rule {
                    Rule::Simple {
                        pattern,
                        token_type,
                        action,
                    } => {
                        if let Ok(Some(m)) = pattern.find(&input[pos..]) {
                            if m.start() != 0 {
                                continue;
                            }
                            let text = m.as_str().to_string();
                            if text.is_empty() {
                                continue;
                            }
                            tokens.push(Token {
                                token_type: *token_type,
                                value: text,
                            });
                            pos += m.end() - m.start();
                            apply_action(action, &mut state_stack, &self.states);
                            matched = true;
                            break;
                        }
                    }
                    Rule::ByGroups {
                        pattern,
                        group_types,
                        action,
                    } => {
                        if let Ok(Some(caps)) = pattern.captures(&input[pos..]) {
                            let full = caps.get(0).unwrap();
                            if full.start() != 0 {
                                continue;
                            }
                            if full.as_str().is_empty() {
                                continue;
                            }
                            for (i, tt) in group_types.iter().enumerate() {
                                if let Some(g) = caps.get(i + 1) {
                                    let val = g.as_str();
                                    if val.is_empty() && *tt == TokenType::Text {
                                        continue; // Skip empty Text groups
                                    }
                                    tokens.push(Token {
                                        token_type: *tt,
                                        value: val.to_string(),
                                    });
                                }
                            }
                            pos += full.end() - full.start();
                            apply_action(action, &mut state_stack, &self.states);
                            matched = true;
                            break;
                        }
                    }
                    Rule::Include(_) => {
                        // Already expanded in collect_rules
                    }
                }
            }

            if !matched {
                // Emit one character as Text and advance
                let ch = &input[pos..pos + input[pos..].chars().next().unwrap().len_utf8()];
                tokens.push(Token {
                    token_type: TokenType::Text,
                    value: ch.to_string(),
                });
                pos += ch.len();
            }
        }

        tokens
    }
}

fn apply_action(action: &Action, stack: &mut Vec<usize>, states: &[State]) {
    match action {
        Action::None => {}
        Action::Push(state_name) => {
            if let Some(idx) = states.iter().position(|s| s.name == *state_name) {
                stack.push(idx);
            }
        }
        Action::Pop => {
            if stack.len() > 1 {
                stack.pop();
            }
        }
    }
}

/// Helper to build a Regex anchored at the start.
pub fn re(pattern: &str) -> Regex {
    // Chroma patterns are anchored at the current position.
    // We prepend ^ if not already there, preserving inline flags.
    let p = if pattern.starts_with('^') || pattern.starts_with("\\A") {
        pattern.to_string()
    } else if is_inline_flags(pattern) {
        // Inline flags like (?s) or (?m) — insert ^ after the flags group
        let close = pattern.find(')').unwrap();
        format!("{}^{}", &pattern[..close + 1], &pattern[close + 1..])
    } else {
        format!("^(?:{})", pattern)
    };
    Regex::new(&p).unwrap_or_else(|e| panic!("bad regex {}: {}", pattern, e))
}

/// Check if pattern starts with an inline flags group like (?s) or (?m),
/// NOT a non-capturing group (?:...) or other group construct.
fn is_inline_flags(pattern: &str) -> bool {
    if !pattern.starts_with("(?") {
        return false;
    }
    // Inline flags: (?s), (?m), (?i), (?x), (?smix), etc.
    // Non-capturing group: (?:...)
    // The third character distinguishes them.
    let bytes = pattern.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    let c = bytes[2];
    // Flags are letters like s, m, i, x
    // Non-capturing group starts with :
    // Lookahead/behind start with = or !
    matches!(c, b's' | b'm' | b'i' | b'x')
}

/// Helper: build regex for `Words(prefix, suffix, words...)`.
pub fn words(prefix: &str, suffix: &str, words: &[&str]) -> String {
    let joined = words.join("|");
    format!("{}(?:{}){}", prefix, joined, suffix)
}
