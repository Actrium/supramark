//! Bash/sh lexer matching chroma's bash.xml rules.

use crate::chroma::engine::*;

pub struct BashLexer {
    inner: StateMachineLexer,
}

impl BashLexer {
    pub fn new() -> Self {
        BashLexer {
            inner: build_bash_lexer(),
        }
    }
}

impl Lexer for BashLexer {
    fn tokenize(&self, source: &str) -> Vec<Token> {
        self.inner.tokenize(source)
    }
}

fn build_bash_lexer() -> StateMachineLexer {
    let bash_builtins = &[
        "alias", "bg", "bind", "break", "builtin", "caller", "cd", "command", "compgen",
        "complete", "declare", "dirs", "disown", "echo", "enable", "eval", "exec", "exit",
        "export", "false", "fc", "fg", "getopts", "hash", "help", "history", "jobs", "kill", "let",
        "local", "logout", "popd", "printf", "pushd", "pwd", "read", "readonly", "set", "shift",
        "shopt", "source", "suspend", "test", "time", "times", "trap", "true", "type", "typeset",
        "ulimit", "umask", "unalias", "unset", "wait",
    ];
    let _builtins_pattern = words("", r"(?=[\s)`])", bash_builtins);

    let bash_keywords = &[
        "if", "fi", "else", "while", "do", "done", "for", "then", "return", "function", "case",
        "select", "continue", "until", "esac", "elif",
    ];
    let _keywords_pattern = format!(r"\b({})\b", bash_keywords.join("|"));

    // State: basic
    let basic_rules = vec![
        Rule::ByGroups {
            pattern: re(&format!(r"\b({})\b(\s*)", bash_keywords.join("|"))),
            group_types: vec![TokenType::Keyword, TokenType::Text],
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(&format!(r"\b(?:{})(?=[\s)`])", bash_builtins.join("|"))),
            token_type: TokenType::NameBuiltin,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\A#!.+\n"),
            token_type: TokenType::CommentPreproc,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"#.*(\S|$)"),
            token_type: TokenType::CommentSingle,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\\[\w\W]"),
            token_type: TokenType::LiteralStringEscape,
            action: Action::None,
        },
        Rule::ByGroups {
            pattern: re(r"(\b\w+)(\s*)(\+?=)"),
            group_types: vec![
                TokenType::NameVariable,
                TokenType::Text,
                TokenType::Operator,
            ],
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"[\[\]{}()=]"),
            token_type: TokenType::Operator,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"<<<"),
            token_type: TokenType::Operator,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"<<-?\s*('?)\\?(\w+)[\w\W]+?\2"),
            token_type: TokenType::LiteralString,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"&&|\|\|"),
            token_type: TokenType::Operator,
            action: Action::None,
        },
    ];

    // State: interp
    let interp_rules = vec![
        Rule::Simple {
            pattern: re(r"\$\(\("),
            token_type: TokenType::Keyword,
            action: Action::Push("math".to_string()),
        },
        Rule::Simple {
            pattern: re(r"\$\("),
            token_type: TokenType::Keyword,
            action: Action::Push("paren".to_string()),
        },
        Rule::Simple {
            pattern: re(r"\$\{#?"),
            token_type: TokenType::LiteralStringInterpol,
            action: Action::Push("curly".to_string()),
        },
        Rule::Simple {
            pattern: re(r"\$[a-zA-Z_]\w*"),
            token_type: TokenType::NameVariable,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\$(?:\d+|[#$?!_*@-])"),
            token_type: TokenType::NameVariable,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\$"),
            token_type: TokenType::Text,
            action: Action::None,
        },
    ];

    // State: data
    let data_rules = vec![
        // $"..." double-quoted string (with optional $ prefix, fully quoted)
        Rule::Simple {
            pattern: re(r#"(?s)\$?"(\\\\|\\[0-7]+|\\.|[^"\\$])*""#),
            token_type: TokenType::LiteralStringDouble,
            action: Action::None,
        },
        // Opening " for interpolated string
        Rule::Simple {
            pattern: re(r#"""#),
            token_type: TokenType::LiteralStringDouble,
            action: Action::Push("string".to_string()),
        },
        // $'...' single-quoted string
        Rule::Simple {
            pattern: re(r"(?s)\$'(\\\\|\\[0-7]+|\\.|[^'\\])*'"),
            token_type: TokenType::LiteralStringSingle,
            action: Action::None,
        },
        // '...' single-quoted string
        Rule::Simple {
            pattern: re(r"(?s)'.*?'"),
            token_type: TokenType::LiteralStringSingle,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r";"),
            token_type: TokenType::Punctuation,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"&"),
            token_type: TokenType::Punctuation,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\|"),
            token_type: TokenType::Punctuation,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\s+"),
            token_type: TokenType::Text,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+(?= |$)"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r#"[^=\s\[\]{}()\$"'`\\<&|;]+"#),
            token_type: TokenType::Text,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"<"),
            token_type: TokenType::Text,
            action: Action::None,
        },
    ];

    // State: string
    let string_rules = vec![
        Rule::Simple {
            pattern: re(r#"""#),
            token_type: TokenType::LiteralStringDouble,
            action: Action::Pop,
        },
        Rule::Simple {
            pattern: re(r#"(?s)(\\\\|\\[0-7]+|\\.|[^"\\$])+"#),
            token_type: TokenType::LiteralStringDouble,
            action: Action::None,
        },
        // Include interp rules inline (we handle this via the Include mechanism)
        Rule::Include("interp".to_string()),
    ];

    // State: root = include basic + backtick + include data + include interp
    let root_rules = vec![
        Rule::Include("basic".to_string()),
        Rule::Simple {
            pattern: re(r"`"),
            token_type: TokenType::LiteralStringBacktick,
            action: Action::Push("backticks".to_string()),
        },
        Rule::Include("data".to_string()),
        Rule::Include("interp".to_string()),
    ];

    // State: backticks
    let backtick_rules = vec![
        Rule::Simple {
            pattern: re(r"`"),
            token_type: TokenType::LiteralStringBacktick,
            action: Action::Pop,
        },
        Rule::Include("root".to_string()),
    ];

    // State: paren
    let paren_rules = vec![
        Rule::Simple {
            pattern: re(r"\)"),
            token_type: TokenType::Keyword,
            action: Action::Pop,
        },
        Rule::Include("root".to_string()),
    ];

    // State: math
    let math_rules = vec![
        Rule::Simple {
            pattern: re(r"\)\)"),
            token_type: TokenType::Keyword,
            action: Action::Pop,
        },
        Rule::Simple {
            pattern: re(r"[-+*/%^|&]|\*\*|\|\|"),
            token_type: TokenType::Operator,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+#\d+"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+#(?! )"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\d+"),
            token_type: TokenType::LiteralNumber,
            action: Action::None,
        },
        Rule::Include("root".to_string()),
    ];

    // State: curly
    let curly_rules = vec![
        Rule::Simple {
            pattern: re(r"\}"),
            token_type: TokenType::LiteralStringInterpol,
            action: Action::Pop,
        },
        Rule::Simple {
            pattern: re(r":-"),
            token_type: TokenType::Keyword,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r"\w+"),
            token_type: TokenType::NameVariable,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r#"[^}:"'`$\\]+"#),
            token_type: TokenType::Punctuation,
            action: Action::None,
        },
        Rule::Simple {
            pattern: re(r":"),
            token_type: TokenType::Punctuation,
            action: Action::None,
        },
        Rule::Include("root".to_string()),
    ];

    StateMachineLexer {
        states: vec![
            State {
                name: "root".to_string(),
                rules: root_rules,
            },
            State {
                name: "basic".to_string(),
                rules: basic_rules,
            },
            State {
                name: "data".to_string(),
                rules: data_rules,
            },
            State {
                name: "interp".to_string(),
                rules: interp_rules,
            },
            State {
                name: "string".to_string(),
                rules: string_rules,
            },
            State {
                name: "backticks".to_string(),
                rules: backtick_rules,
            },
            State {
                name: "paren".to_string(),
                rules: paren_rules,
            },
            State {
                name: "math".to_string(),
                rules: math_rules,
            },
            State {
                name: "curly".to_string(),
                rules: curly_rules,
            },
        ],
        ensure_nl: false, // bash.xml doesn't set EnsureNL
    }
}
