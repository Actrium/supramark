//! Python 3 language lexer, matching chroma's Python3 lexer rules.

use crate::chroma::engine::*;

pub struct PythonLexer {
    inner: StateMachineLexer,
}

impl PythonLexer {
    pub fn new() -> Self {
        PythonLexer {
            inner: build_python_lexer(),
        }
    }
}

impl Lexer for PythonLexer {
    fn tokenize(&self, source: &str) -> Vec<Token> {
        self.inner.tokenize(source)
    }
}

fn build_python_lexer() -> StateMachineLexer {
    // Note: "def" and "class" are handled by ByGroups rules to capture the
    // following name as NameFunction, so they are NOT in this keyword list.
    let keywords = words(
        "",
        r"\b",
        &[
            "assert", "async", "await", "break", "continue", "del", "elif", "else", "except",
            "finally", "for", "global", "if", "lambda", "pass", "raise", "return", "try", "while",
            "with", "yield",
        ],
    );

    let keyword_constants = words("", r"\b", &["True", "False", "None"]);

    let keyword_namespace = words("", r"\b", &["import", "from"]);

    let builtins = &[
        "abs",
        "all",
        "any",
        "ascii",
        "bin",
        "bool",
        "breakpoint",
        "bytearray",
        "bytes",
        "callable",
        "chr",
        "classmethod",
        "compile",
        "complex",
        "delattr",
        "dict",
        "dir",
        "divmod",
        "enumerate",
        "eval",
        "exec",
        "filter",
        "float",
        "format",
        "frozenset",
        "getattr",
        "globals",
        "hasattr",
        "hash",
        "help",
        "hex",
        "id",
        "input",
        "int",
        "isinstance",
        "issubclass",
        "iter",
        "len",
        "list",
        "locals",
        "map",
        "max",
        "memoryview",
        "min",
        "next",
        "object",
        "oct",
        "open",
        "ord",
        "pow",
        "print",
        "property",
        "range",
        "repr",
        "reversed",
        "round",
        "set",
        "setattr",
        "slice",
        "sorted",
        "staticmethod",
        "str",
        "sum",
        "super",
        "tuple",
        "type",
        "vars",
        "zip",
        "__import__",
    ];
    let builtin_words = words("", r"\b", builtins);

    let root_rules = vec![
        // Whitespace
        Rule::Simple {
            pattern: re(r"\n"),
            token_type: TokenType::Text,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\s+"),
            token_type: TokenType::Text,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\\\n"),
            token_type: TokenType::Text,
            action: Action::None,
        },
        // Comments
        Rule::Simple {
            pattern: re(r"#[^\n]*"),
            token_type: TokenType::CommentSingle,
            action: Action::None,
        },
        // Decorators
        Rule::Simple {
            pattern: re(r"@\w+"),
            token_type: TokenType::NameFunction,
            action: Action::None,
        },
        // Triple-quoted strings (must come before single-quoted)
        Rule::Simple {
            pattern: re(r#"[fFbBuUrR]{0,2}"""(?:[^"\\]|\\.|"(?!""))*""""#),
            token_type: TokenType::LiteralStringDouble,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"[fFbBuUrR]{0,2}'''(?:[^'\\]|\\.|'(?!''))*'''"),
            token_type: TokenType::LiteralStringSingle,
            action: Action::None,
        },
        // String prefix (f, r, b, etc.) + double-quoted string
        Rule::ByGroups {
            pattern: re(r#"([fFbBuUrR]{1,2})(")([^"\\]*(?:\\.[^"\\]*)*)(")?"#),
            group_types: vec![
                TokenType::LiteralStringAffix,
                TokenType::LiteralStringDouble,
                TokenType::LiteralStringDouble,
                TokenType::LiteralStringDouble,
            ],
            action: Action::None,
        },
        // String prefix + single-quoted string
        Rule::ByGroups {
            pattern: re(r"([fFbBuUrR]{1,2})(')([^'\\]*(?:\\.[^'\\]*)*)(')?"),
            group_types: vec![
                TokenType::LiteralString,
                TokenType::LiteralStringSingle,
                TokenType::LiteralStringSingle,
                TokenType::LiteralStringSingle,
            ],
            action: Action::None,
        },
        // Plain double-quoted string (no prefix)
        // Chroma emits an empty LiteralStringAffix first, then splits body
        Rule::ByGroups {
            pattern: re(r#"()(")([^"\\]*(?:\\.[^"\\]*)*)(")?"#),
            group_types: vec![
                TokenType::LiteralStringAffix,
                TokenType::LiteralStringDouble,
                TokenType::LiteralStringDouble,
                TokenType::LiteralStringDouble,
            ],
            action: Action::None,
        },
        // Plain single-quoted string (no prefix)
        Rule::ByGroups {
            pattern: re(r"()(')([^'\\]*(?:\\.[^'\\]*)*)(')?"),
            group_types: vec![
                TokenType::LiteralString,
                TokenType::LiteralStringSingle,
                TokenType::LiteralStringSingle,
                TokenType::LiteralStringSingle,
            ],
            action: Action::None,
        },
        // Function definition: def name — must come before keywords
        Rule::ByGroups {
            pattern: re(r"\b(def)(\s+)([a-zA-Z_]\w*)"),
            group_types: vec![TokenType::Keyword, TokenType::Text, TokenType::NameFunction],
            action: Action::None,
        },
        // Class definition: class Name — must come before keywords
        Rule::ByGroups {
            pattern: re(r"\b(class)(\s+)([a-zA-Z_]\w*)"),
            group_types: vec![TokenType::Keyword, TokenType::Text, TokenType::NameFunction],
            action: Action::None,
        },
        // Standalone def/class (without following name)
        Rule::Simple {
            pattern: re(r"\b(def|class)\b"),
            token_type: TokenType::Keyword,
            action: Action::None,
        },
        // Keyword namespace: import, from
        Rule::Simple {
            pattern: re(&keyword_namespace),
            token_type: TokenType::KeywordNamespace,
            action: Action::None,
        },
        // Keywords
        Rule::Simple {
            pattern: re(&keywords),
            token_type: TokenType::Keyword,
            action: Action::None,
        },
        // Keyword constants: True, False, None
        Rule::Simple {
            pattern: re(&keyword_constants),
            token_type: TokenType::KeywordConstant,
            action: Action::None,
        },
        // Operator words: in, not, and, or, is
        Rule::Simple {
            pattern: re(r"\b(not|in|and|or|is)\b"),
            token_type: TokenType::Operator,
            action: Action::None,
        },
        // Builtins
        Rule::Simple {
            pattern: re(&builtin_words),
            token_type: TokenType::NameBuiltin,
            action: Action::None,
        },
        // Numbers
        Rule::Simple {
            pattern: re(r"0[xX][0-9a-fA-F_]+"),
            token_type: TokenType::LiteralNumberHex,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"0[oO][0-7_]+"),
            token_type: TokenType::LiteralNumberOct,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"0[bB][01_]+"),
            token_type: TokenType::LiteralNumberBin,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+\.\d*([eE][+-]?\d+)?j?"),
            token_type: TokenType::LiteralNumberFloat,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\.\d+([eE][+-]?\d+)?j?"),
            token_type: TokenType::LiteralNumberFloat,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+[eE][+-]?\d+j?"),
            token_type: TokenType::LiteralNumberFloat,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+j"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"(0|[1-9][0-9_]*)"),
            token_type: TokenType::LiteralNumberInteger,
            action: Action::None,
        },
        // Operators
        Rule::Simple {
            pattern: re(r"(//=?|\*\*=?|>>=?|<<=?|<>|!=|>=|<=|[-+*/%&|^=<>]=?|~)"),
            token_type: TokenType::Operator,
            action: Action::None,
        },
        // Punctuation
        Rule::Simple {
            pattern: re(r"[(){}\[\]:.,;@]"),
            token_type: TokenType::Punctuation,
            action: Action::None,
        },
        // Identifiers
        Rule::Simple {
            pattern: re(r"[a-zA-Z_]\w*"),
            token_type: TokenType::NameOther,
            action: Action::None,
        },
    ];

    StateMachineLexer {
        states: vec![State {
            name: "root".to_string(),
            rules: root_rules,
        }],
        ensure_nl: true,
    }
}
