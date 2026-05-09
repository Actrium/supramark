//! Go language lexer, matching chroma's Go lexer rules.

use crate::chroma::engine::*;

pub struct GoLexer {
    inner: StateMachineLexer,
}

impl GoLexer {
    pub fn new() -> Self {
        GoLexer {
            inner: build_go_lexer(),
        }
    }
}

impl Lexer for GoLexer {
    fn tokenize(&self, source: &str) -> Vec<Token> {
        self.inner.tokenize(source)
    }
}

fn build_go_lexer() -> StateMachineLexer {
    let builtin_types = &[
        "uint",
        "uint8",
        "uint16",
        "uint32",
        "uint64",
        "int",
        "int8",
        "int16",
        "int32",
        "int64",
        "float",
        "float32",
        "float64",
        "complex64",
        "complex128",
        "byte",
        "rune",
        "string",
        "bool",
        "error",
        "uintptr",
        "print",
        "println",
        "panic",
        "recover",
        "close",
        "complex",
        "real",
        "imag",
        "len",
        "cap",
        "append",
        "copy",
        "delete",
        "new",
        "make",
        "clear",
        "min",
        "max",
    ];

    // Words("", `\b(\()`, ...) in Go chroma wraps the alternation in a
    // capture group so ByGroups can assign NameBuiltin to group 1 and
    // Punctuation to group 2.
    let builtin_words = builtin_types.to_vec().join("|");
    let builtin_types_with_paren = format!("((?:{})\\b)(\\()", builtin_words);
    let builtin_types_plain = words(
        "",
        r"\b",
        &[
            "uint",
            "uint8",
            "uint16",
            "uint32",
            "uint64",
            "int",
            "int8",
            "int16",
            "int32",
            "int64",
            "float",
            "float32",
            "float64",
            "complex64",
            "complex128",
            "byte",
            "rune",
            "string",
            "bool",
            "error",
            "uintptr",
        ],
    );

    let flow_keywords = words(
        "",
        r"\b",
        &[
            "break",
            "default",
            "select",
            "case",
            "defer",
            "go",
            "else",
            "goto",
            "switch",
            "fallthrough",
            "if",
            "range",
            "continue",
            "for",
            "return",
        ],
    );

    let root_rules = vec![
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
        Rule::Simple {
            pattern: re(r"//(.*?)\n"),
            token_type: TokenType::CommentSingle,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"(?s)/(\\\n)?\*(.|\n)*?\*(\\\n)?/"),
            token_type: TokenType::CommentMultiline,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"(import|package)\b"),
            token_type: TokenType::KeywordNamespace,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"(var|func|struct|map|chan|type|interface|const)\b"),
            token_type: TokenType::KeywordDeclaration,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(&flow_keywords),
            token_type: TokenType::Keyword,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"(true|false|iota|nil)\b"),
            token_type: TokenType::KeywordConstant,
            action: Action::None,
        },
        // ByGroups: builtin(paren) — e.g. panic(
        Rule::ByGroups {
            pattern: re(&builtin_types_with_paren),
            group_types: vec![TokenType::NameBuiltin, TokenType::Punctuation],
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(&builtin_types_plain),
            token_type: TokenType::KeywordType,
            action: Action::None,
        },
        // Numbers
        Rule::Simple {
            pattern: re(r"\d+i"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+\.\d*([Ee][-+]\d+)?i"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\.\d+([Ee][-+]\d+)?i"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+[Ee][-+]\d+i"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+(\.\d+[eE][+\-]?\d+|\.\d*|[eE][+\-]?\d+)"),
            token_type: TokenType::LiteralNumberFloat,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\.\d+([eE][+\-]?\d+)?"),
            token_type: TokenType::LiteralNumberFloat,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"0[0-7]+"),
            token_type: TokenType::LiteralNumberOct,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"0[xX][0-9a-fA-F_]+"),
            token_type: TokenType::LiteralNumberHex,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"0b[01_]+"),
            token_type: TokenType::LiteralNumberBin,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"(0|[1-9][0-9_]*)"),
            token_type: TokenType::LiteralNumberInteger,
            action: Action::None,
        },
        // Character literal
        Rule::Simple {
            pattern: re(
                r"'(\\['\x22\\abfnrtv]|\\x[0-9a-fA-F]{2}|\\[0-7]{1,3}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}|[^\\])'",
            ),
            token_type: TokenType::LiteralStringChar,
            action: Action::None,
        },
        // Backtick string (raw string, simplified — no template expansion)
        Rule::Simple {
            pattern: re(r"`[^`]*`"),
            token_type: TokenType::LiteralString,
            action: Action::None,
        },
        // Double-quoted string
        Rule::Simple {
            pattern: re(r#""(\\\\|\\"|[^"])*""#),
            token_type: TokenType::LiteralString,
            action: Action::None,
        },
        // Operators
        Rule::Simple {
            pattern: re(
                r"(<<=|>>=|<<|>>|<=|>=|&\^=|&\^|\+=|-=|\*=|/=|%=|&=|\|=|&&|\|\||<-|\+\+|--|==|!=|:=|\.\.\.|[+\-*/%&])",
            ),
            token_type: TokenType::Operator,
            action: Action::None,
        },
        // Function call: name ( — ByGroups
        Rule::ByGroups {
            pattern: re(r"([a-zA-Z_]\w*)(\s*)(\()"),
            group_types: vec![
                TokenType::NameFunction,
                TokenType::Text,
                TokenType::Punctuation,
            ],
            action: Action::None,
        },
        // Punctuation
        Rule::Simple {
            pattern: re(r"[|^<>=!()\[\]{}.,;:~]"),
            token_type: TokenType::Punctuation,
            action: Action::None,
        },
        // Identifiers
        Rule::Simple {
            pattern: re(r"[^\W\d]\w*"),
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
