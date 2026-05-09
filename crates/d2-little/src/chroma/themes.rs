//! Theme color mappings for github (light) and catppuccin-mocha (dark).
//! Data extracted from chroma v2.14.0.

use crate::chroma::engine::TokenType;
use std::collections::HashMap;

/// A single style entry for a token type.
#[derive(Debug, Clone)]
pub struct StyleEntry {
    pub color: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl StyleEntry {
    pub fn is_zero(&self) -> bool {
        self.color.is_none() && !self.bold && !self.italic && !self.underline
    }
}

/// A theme: background color + per-token-type styles.
pub struct Theme {
    pub background: String,
    entries: HashMap<TokenType, StyleEntry>,
}

impl Theme {
    pub fn get(&self, tt: TokenType) -> Option<&StyleEntry> {
        self.entries.get(&tt)
    }
}

pub fn get_theme(name: &str) -> Option<Theme> {
    match name {
        "github" => Some(build_github()),
        "catppuccin-mocha" => Some(build_catppuccin_mocha()),
        _ => None,
    }
}

fn entry(color: &str, bold: bool, italic: bool) -> StyleEntry {
    StyleEntry {
        color: Some(color.to_string()),
        bold,
        italic,
        underline: false,
    }
}

fn build_github() -> Theme {
    let mut m = HashMap::new();

    // From chroma's github style dump
    m.insert(TokenType::Comment, entry("#999988", false, true));
    m.insert(TokenType::CommentSingle, entry("#999988", false, true));
    m.insert(TokenType::CommentMultiline, entry("#999988", false, true));
    m.insert(TokenType::CommentPreproc, entry("#999999", true, true));

    m.insert(TokenType::Keyword, entry("#000000", true, false));
    m.insert(TokenType::KeywordConstant, entry("#000000", true, false));
    m.insert(TokenType::KeywordDeclaration, entry("#000000", true, false));
    m.insert(TokenType::KeywordNamespace, entry("#000000", true, false));
    m.insert(TokenType::KeywordType, entry("#445588", true, false));

    m.insert(TokenType::NameBuiltin, entry("#0086b3", false, false));
    m.insert(TokenType::NameFunction, entry("#990000", true, false));
    m.insert(TokenType::NameVariable, entry("#008080", false, false));

    m.insert(TokenType::Operator, entry("#000000", true, false));

    m.insert(TokenType::LiteralString, entry("#dd1144", false, false));
    m.insert(TokenType::LiteralStringChar, entry("#dd1144", false, false));
    m.insert(
        TokenType::LiteralStringDouble,
        entry("#dd1144", false, false),
    );
    m.insert(
        TokenType::LiteralStringSingle,
        entry("#dd1144", false, false),
    );
    m.insert(
        TokenType::LiteralStringBacktick,
        entry("#dd1144", false, false),
    );
    m.insert(
        TokenType::LiteralStringEscape,
        entry("#dd1144", false, false),
    );
    m.insert(
        TokenType::LiteralStringInterpol,
        entry("#dd1144", false, false),
    );
    m.insert(
        TokenType::LiteralStringAffix,
        entry("#dd1144", false, false),
    );

    m.insert(TokenType::LiteralNumber, entry("#009999", false, false));
    m.insert(
        TokenType::LiteralNumberFloat,
        entry("#009999", false, false),
    );
    m.insert(TokenType::LiteralNumberHex, entry("#009999", false, false));
    m.insert(
        TokenType::LiteralNumberInteger,
        entry("#009999", false, false),
    );
    m.insert(TokenType::LiteralNumberOct, entry("#009999", false, false));
    m.insert(TokenType::LiteralNumberBin, entry("#009999", false, false));

    Theme {
        background: "#ffffff".to_string(),
        entries: m,
    }
}

fn build_catppuccin_mocha() -> Theme {
    let mut m = HashMap::new();

    m.insert(TokenType::Text, entry("#cdd6f4", false, false));
    m.insert(TokenType::Other, entry("#cdd6f4", false, false));

    m.insert(TokenType::Comment, entry("#6c7086", false, true));
    m.insert(TokenType::CommentSingle, entry("#6c7086", false, true));
    m.insert(TokenType::CommentMultiline, entry("#6c7086", false, true));
    m.insert(TokenType::CommentPreproc, entry("#6c7086", false, true));

    m.insert(TokenType::Keyword, entry("#cba6f7", false, false));
    m.insert(TokenType::KeywordConstant, entry("#fab387", false, false));
    m.insert(
        TokenType::KeywordDeclaration,
        entry("#f38ba8", false, false),
    );
    m.insert(TokenType::KeywordNamespace, entry("#94e2d5", false, false));
    m.insert(TokenType::KeywordType, entry("#f38ba8", false, false));

    m.insert(TokenType::Name, entry("#cdd6f4", false, false));
    m.insert(TokenType::NameBuiltin, entry("#89dceb", false, false));
    m.insert(TokenType::NameFunction, entry("#89b4fa", false, false));
    m.insert(TokenType::NameOther, entry("#cdd6f4", false, false));
    m.insert(TokenType::NameVariable, entry("#f5e0dc", false, false));

    m.insert(TokenType::Literal, entry("#cdd6f4", false, false));
    m.insert(TokenType::Operator, entry("#89dceb", true, false));

    m.insert(TokenType::Punctuation, entry("#cdd6f4", false, false));

    m.insert(TokenType::LiteralString, entry("#a6e3a1", false, false));
    m.insert(TokenType::LiteralStringChar, entry("#a6e3a1", false, false));
    m.insert(
        TokenType::LiteralStringDouble,
        entry("#a6e3a1", false, false),
    );
    m.insert(
        TokenType::LiteralStringSingle,
        entry("#a6e3a1", false, false),
    );
    m.insert(
        TokenType::LiteralStringBacktick,
        entry("#a6e3a1", false, false),
    );
    m.insert(
        TokenType::LiteralStringEscape,
        entry("#89b4fa", false, false),
    );
    m.insert(
        TokenType::LiteralStringInterpol,
        entry("#a6e3a1", false, false),
    );
    m.insert(
        TokenType::LiteralStringAffix,
        entry("#f38ba8", false, false),
    );

    m.insert(TokenType::LiteralNumber, entry("#fab387", false, false));
    m.insert(
        TokenType::LiteralNumberFloat,
        entry("#fab387", false, false),
    );
    m.insert(TokenType::LiteralNumberHex, entry("#fab387", false, false));
    m.insert(
        TokenType::LiteralNumberInteger,
        entry("#fab387", false, false),
    );
    m.insert(TokenType::LiteralNumberOct, entry("#fab387", false, false));
    m.insert(TokenType::LiteralNumberBin, entry("#fab387", false, false));

    Theme {
        background: "#1e1e2e".to_string(),
        entries: m,
    }
}
