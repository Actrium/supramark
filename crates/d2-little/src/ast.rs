//! d2-ast: Abstract syntax tree types for the d2 language.
//!
//! Ported from the Go d2ast package.

use std::collections::HashSet;
use std::fmt;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Position & Range
// ---------------------------------------------------------------------------

/// A zero-indexed line:column:byte position in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    /// Byte offset. `usize::MAX` is used as sentinel for "missing".
    pub byte: usize,
}

impl Position {
    pub fn new(line: usize, column: usize, byte: usize) -> Self {
        Self { line, column, byte }
    }

    /// Advance position by one character.
    pub fn advance(mut self, ch: char) -> Self {
        let size = ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 0;
        } else {
            self.column += size;
        }
        self.byte += size;
        self
    }

    /// Advance position by an entire string.
    pub fn advance_string(mut self, s: &str) -> Self {
        for ch in s.chars() {
            self = self.advance(ch);
        }
        self
    }

    /// Subtract one character (inverse of advance, panics on newline).
    pub fn subtract(mut self, ch: char) -> Self {
        let size = ch.len_utf8();
        assert!(ch != '\n', "cannot subtract newline from Position");
        self.column -= size;
        self.byte -= size;
        self
    }

    pub fn subtract_string(mut self, s: &str) -> Self {
        for ch in s.chars() {
            self = self.subtract(ch);
        }
        self
    }

    pub fn before(&self, other: &Position) -> bool {
        if self.byte != other.byte && self.byte != usize::MAX && other.byte != usize::MAX {
            return self.byte < other.byte;
        }
        if self.line != other.line {
            return self.line < other.line;
        }
        self.column < other.column
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line + 1, self.column + 1)
    }
}

/// A source range: path + start..end positions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Range {
    pub path: String,
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(path: impl Into<String>, start: Position, end: Position) -> Self {
        Self {
            path: path.into(),
            start,
            end,
        }
    }

    pub fn one_line(&self) -> bool {
        self.start.line == self.end.line
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.path.is_empty() {
            write!(f, "{}:", self.path)?;
        }
        write!(f, "{}", self.start)
    }
}

// ---------------------------------------------------------------------------
// Error (used by both parser and compiler)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub range: Range,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

// ---------------------------------------------------------------------------
// AST Node types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub range: Range,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockComment {
    pub range: Range,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Null {
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Suspension {
    pub range: Range,
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Boolean {
    pub range: Range,
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    pub range: Range,
    pub raw: String,
    /// Stored as f64 for simplicity (Go uses *big.Rat).
    pub value: f64,
}

/// Either a literal string fragment or a substitution `${...}`.
#[derive(Debug, Clone, PartialEq)]
pub struct InterpolationBox {
    pub string: Option<String>,
    pub string_raw: Option<String>,
    pub substitution: Option<Substitution>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnquotedString {
    pub range: Range,
    pub value: Vec<InterpolationBox>,
    /// Parsed glob pattern segments (if this is a key with glob).
    pub pattern: Option<Vec<String>>,
}

impl UnquotedString {
    pub fn scalar_string(&self) -> &str {
        if let Some(first) = self.value.first()
            && let Some(ref s) = first.string
        {
            return s.as_str();
        }
        ""
    }

    pub fn set_string(&mut self, s: String) {
        self.value = vec![InterpolationBox {
            string: Some(s),
            string_raw: None,
            substitution: None,
        }];
    }
}

/// Helper to create a simple `UnquotedString` from a plain &str.
pub fn flat_unquoted_string(s: &str) -> UnquotedString {
    UnquotedString {
        range: Range::default(),
        value: vec![InterpolationBox {
            string: Some(s.to_string()),
            string_raw: None,
            substitution: None,
        }],
        pattern: None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoubleQuotedString {
    pub range: Range,
    pub value: Vec<InterpolationBox>,
}

impl DoubleQuotedString {
    pub fn scalar_string(&self) -> &str {
        if let Some(first) = self.value.first()
            && let Some(ref s) = first.string
        {
            return s.as_str();
        }
        ""
    }

    pub fn set_string(&mut self, s: String) {
        self.value = vec![InterpolationBox {
            string: Some(s),
            string_raw: None,
            substitution: None,
        }];
    }
}

pub fn flat_double_quoted_string(s: &str) -> DoubleQuotedString {
    DoubleQuotedString {
        range: Range::default(),
        value: vec![InterpolationBox {
            string: Some(s.to_string()),
            string_raw: None,
            substitution: None,
        }],
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SingleQuotedString {
    pub range: Range,
    pub raw: String,
    pub value: String,
}

impl SingleQuotedString {
    pub fn scalar_string(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockString {
    pub range: Range,
    pub quote: String,
    pub tag: String,
    pub value: String,
}

impl BlockString {
    pub fn scalar_string(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Substitution {
    pub range: Range,
    pub spread: bool,
    pub path: Vec<StringBox>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub range: Range,
    pub spread: bool,
    pub pre: String,
    pub path: Vec<StringBox>,
}

// ---------------------------------------------------------------------------
// Composite types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub range: Range,
    pub nodes: Vec<ArrayNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    pub range: Range,
    pub nodes: Vec<MapNode>,
}

impl Map {
    pub fn is_file_map(&self) -> bool {
        self.range.start.line == 0 && self.range.start.column == 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub range: Range,
    pub ampersand: bool,
    pub not_ampersand: bool,
    pub key: Option<KeyPath>,
    pub edges: Vec<Edge>,
    pub edge_index: Option<EdgeIndex>,
    pub edge_key: Option<KeyPath>,
    pub primary: Option<ScalarBox>,
    pub value: Option<ValueBox>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyPath {
    pub range: Range,
    pub path: Vec<StringBox>,
}

impl KeyPath {
    pub fn string_ida(&self) -> Vec<String> {
        self.path
            .iter()
            .map(|sb| sb.scalar_string().to_string())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub range: Range,
    pub src: Option<KeyPath>,
    pub src_arrow: String,
    pub dst: Option<KeyPath>,
    pub dst_arrow: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeIndex {
    pub range: Range,
    pub int: Option<i64>,
    pub glob: bool,
}

// ---------------------------------------------------------------------------
// String enum (covers all string node kinds)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum StringNode {
    Unquoted(UnquotedString),
    DoubleQuoted(DoubleQuotedString),
    SingleQuoted(SingleQuotedString),
    Block(BlockString),
}

impl StringNode {
    pub fn scalar_string(&self) -> &str {
        match self {
            Self::Unquoted(s) => s.scalar_string(),
            Self::DoubleQuoted(s) => s.scalar_string(),
            Self::SingleQuoted(s) => s.scalar_string(),
            Self::Block(s) => s.scalar_string(),
        }
    }

    pub fn get_range(&self) -> &Range {
        match self {
            Self::Unquoted(s) => &s.range,
            Self::DoubleQuoted(s) => &s.range,
            Self::SingleQuoted(s) => &s.range,
            Self::Block(s) => &s.range,
        }
    }
}

// ---------------------------------------------------------------------------
// Box / enum wrappers
// ---------------------------------------------------------------------------

/// Box for any scalar (leaf) value.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarBox {
    Null(Null),
    Suspension(Suspension),
    Boolean(Boolean),
    Number(Number),
    UnquotedString(UnquotedString),
    DoubleQuotedString(DoubleQuotedString),
    SingleQuotedString(SingleQuotedString),
    BlockString(BlockString),
}

impl ScalarBox {
    pub fn scalar_string(&self) -> String {
        match self {
            Self::Null(_) => String::new(),
            Self::Suspension(_) => String::new(),
            Self::Boolean(b) => b.value.to_string(),
            Self::Number(n) => n.raw.clone(),
            Self::UnquotedString(s) => s.scalar_string().to_string(),
            Self::DoubleQuotedString(s) => s.scalar_string().to_string(),
            Self::SingleQuotedString(s) => s.scalar_string().to_string(),
            Self::BlockString(s) => s.scalar_string().to_string(),
        }
    }

    pub fn get_range(&self) -> &Range {
        match self {
            Self::Null(n) => &n.range,
            Self::Suspension(n) => &n.range,
            Self::Boolean(n) => &n.range,
            Self::Number(n) => &n.range,
            Self::UnquotedString(n) => &n.range,
            Self::DoubleQuotedString(n) => &n.range,
            Self::SingleQuotedString(n) => &n.range,
            Self::BlockString(n) => &n.range,
        }
    }
}

/// Box for any value (scalar, array, map, import).
#[derive(Debug, Clone, PartialEq)]
pub enum ValueBox {
    Null(Null),
    Suspension(Suspension),
    Boolean(Boolean),
    Number(Number),
    UnquotedString(UnquotedString),
    DoubleQuotedString(DoubleQuotedString),
    SingleQuotedString(SingleQuotedString),
    BlockString(BlockString),
    Array(Box<Array>),
    Map(Box<Map>),
    Import(Import),
}

impl ValueBox {
    pub fn scalar_box(&self) -> Option<ScalarBox> {
        match self {
            Self::Null(n) => Some(ScalarBox::Null(n.clone())),
            Self::Suspension(n) => Some(ScalarBox::Suspension(n.clone())),
            Self::Boolean(n) => Some(ScalarBox::Boolean(n.clone())),
            Self::Number(n) => Some(ScalarBox::Number(n.clone())),
            Self::UnquotedString(n) => Some(ScalarBox::UnquotedString(n.clone())),
            Self::DoubleQuotedString(n) => Some(ScalarBox::DoubleQuotedString(n.clone())),
            Self::SingleQuotedString(n) => Some(ScalarBox::SingleQuotedString(n.clone())),
            Self::BlockString(n) => Some(ScalarBox::BlockString(n.clone())),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Self::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null(_) => "null",
            Self::Suspension(_) => "suspension",
            Self::Boolean(_) => "boolean",
            Self::Number(_) => "number",
            Self::UnquotedString(_) => "unquoted string",
            Self::DoubleQuotedString(_) => "double quoted string",
            Self::SingleQuotedString(_) => "single quoted string",
            Self::BlockString(_) => "block string",
            Self::Array(_) => "array",
            Self::Map(_) => "map",
            Self::Import(_) => "import",
        }
    }
}

/// Box for a string (key path element).
#[derive(Debug, Clone, PartialEq)]
pub enum StringBox {
    Unquoted(UnquotedString),
    DoubleQuoted(DoubleQuotedString),
    SingleQuoted(SingleQuotedString),
    Block(BlockString),
}

impl StringBox {
    pub fn scalar_string(&self) -> &str {
        match self {
            Self::Unquoted(s) => s.scalar_string(),
            Self::DoubleQuoted(s) => s.scalar_string(),
            Self::SingleQuoted(s) => s.scalar_string(),
            Self::Block(s) => s.scalar_string(),
        }
    }

    pub fn get_range(&self) -> &Range {
        match self {
            Self::Unquoted(s) => &s.range,
            Self::DoubleQuoted(s) => &s.range,
            Self::SingleQuoted(s) => &s.range,
            Self::Block(s) => &s.range,
        }
    }

    pub fn as_unquoted(&self) -> Option<&UnquotedString> {
        match self {
            Self::Unquoted(s) => Some(s),
            _ => None,
        }
    }
}

/// A node that can appear inside a Map. `Key` is boxed because it is much
/// larger than the other variants (~680 B vs ~128 B), which would otherwise
/// balloon every `MapNode` in the AST.
#[derive(Debug, Clone, PartialEq)]
pub enum MapNode {
    Comment(Comment),
    BlockComment(BlockComment),
    Substitution(Substitution),
    Import(Import),
    Key(Box<Key>),
}

impl MapNode {
    pub fn as_key(&self) -> Option<&Key> {
        match self {
            Self::Key(k) => Some(k),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Comment(_) => "comment",
            Self::BlockComment(_) => "block comment",
            Self::Substitution(_) => "substitution",
            Self::Import(_) => "import",
            Self::Key(_) => "map key",
        }
    }
}

/// A node that can appear inside an Array.
#[derive(Debug, Clone, PartialEq)]
pub enum ArrayNode {
    Comment(Comment),
    BlockComment(BlockComment),
    Substitution(Substitution),
    Import(Import),
    Null(Null),
    Boolean(Boolean),
    Number(Number),
    UnquotedString(UnquotedString),
    DoubleQuotedString(DoubleQuotedString),
    SingleQuotedString(SingleQuotedString),
    BlockString(BlockString),
    Array(Box<Array>),
    Map(Box<Map>),
}

impl ArrayNode {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Comment(_) => "comment",
            Self::BlockComment(_) => "block comment",
            Self::Substitution(_) => "substitution",
            Self::Import(_) => "import",
            Self::Null(_) => "null",
            Self::Boolean(_) => "boolean",
            Self::Number(_) => "number",
            Self::UnquotedString(_) => "unquoted string",
            Self::DoubleQuotedString(_) => "double quoted string",
            Self::SingleQuotedString(_) => "single quoted string",
            Self::BlockString(_) => "block string",
            Self::Array(_) => "array",
            Self::Map(_) => "map",
        }
    }
}

// ---------------------------------------------------------------------------
// Keyword constants / reserved word tables
// ---------------------------------------------------------------------------

pub static BOARD_KEYWORDS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| ["layers", "scenarios", "steps"].into_iter().collect());

pub static STYLE_KEYWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "opacity",
        "stroke",
        "fill",
        "fill-pattern",
        "stroke-width",
        "stroke-dash",
        "border-radius",
        "font",
        "font-size",
        "font-color",
        "bold",
        "italic",
        "underline",
        "text-transform",
        "shadow",
        "multiple",
        "double-border",
        "3d",
        "animated",
        "filled",
    ]
    .into_iter()
    .collect()
});

pub static SIMPLE_RESERVED_KEYWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "label",
        "shape",
        "icon",
        "constraint",
        "tooltip",
        "link",
        "near",
        "width",
        "height",
        "direction",
        "top",
        "left",
        "grid-rows",
        "grid-columns",
        "grid-gap",
        "vertical-gap",
        "horizontal-gap",
        "class",
        "vars",
    ]
    .into_iter()
    .collect()
});

pub static RESERVED_KEYWORD_HOLDERS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| ["style"].into_iter().collect());

pub static COMPOSITE_RESERVED_KEYWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "source-arrowhead",
        "target-arrowhead",
        "classes",
        "constraint",
        "label",
        "icon",
        "tooltip",
        // Also includes holders + board keywords
        "style",
        "layers",
        "scenarios",
        "steps",
    ]
    .into_iter()
    .collect()
});

/// Union of all reserved keywords.
pub static RESERVED_KEYWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    for &k in SIMPLE_RESERVED_KEYWORDS.iter() {
        set.insert(k);
    }
    for &k in STYLE_KEYWORDS.iter() {
        set.insert(k);
    }
    for &k in COMPOSITE_RESERVED_KEYWORDS.iter() {
        set.insert(k);
    }
    set
});

pub static NEAR_CONSTANTS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "top-left",
        "top-center",
        "top-right",
        "center-left",
        "center-right",
        "bottom-left",
        "bottom-center",
        "bottom-right",
    ]
    .into_iter()
    .collect()
});

pub static LABEL_POSITIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "top-left",
        "top-center",
        "top-right",
        "center-left",
        "center-center",
        "center-right",
        "bottom-left",
        "bottom-center",
        "bottom-right",
        "outside-top-left",
        "outside-top-center",
        "outside-top-right",
        "outside-left-top",
        "outside-left-center",
        "outside-left-bottom",
        "outside-right-top",
        "outside-right-center",
        "outside-right-bottom",
        "outside-bottom-left",
        "outside-bottom-center",
        "outside-bottom-right",
        "border-top-left",
        "border-top-center",
        "border-top-right",
        "border-left-top",
        "border-left-center",
        "border-left-bottom",
        "border-right-top",
        "border-right-center",
        "border-right-bottom",
        "border-bottom-left",
        "border-bottom-center",
        "border-bottom-right",
    ]
    .into_iter()
    .collect()
});

pub static FILL_PATTERNS: &[&str] = &["none", "dots", "lines", "grain", "paper"];

pub static TEXT_TRANSFORMS: &[&str] = &["none", "uppercase", "lowercase", "capitalize"];

// ---------------------------------------------------------------------------
// Special character sets for unquoted strings
// ---------------------------------------------------------------------------

pub const UNQUOTED_KEY_SPECIALS: &str = "#;\n\\{}[]'\"|-<>*&()@&";
pub const UNQUOTED_VALUE_SPECIALS: &str = "#;\n\\{}[]'\"|$@";
