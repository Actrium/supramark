//! Verify Go tokenizer output matches chroma for the code_snippet e2e case.

use d2_little::chroma::{TokenType, split_into_lines, tokenize};

const CODE_SNIPPET: &str = r#"// RegisterHash registers a function that returns a new instance of the given
// hash function. This is intended to be called from the init function in
// packages that implement hash functions.
func RegisterHash(h Hash, f func() hash.Hash) {
	if h >= maxHash {
		panic("crypto: RegisterHash of unknown hash function")
	}
	hashes[h] = f
}
"#;

#[test]
fn test_go_tokens_match_chroma() {
    let tokens = tokenize("go", CODE_SNIPPET).unwrap();

    // Expected tokens from Go's chroma dump
    let expected: Vec<(TokenType, &str)> = vec![
        (
            TokenType::CommentSingle,
            "// RegisterHash registers a function that returns a new instance of the given\n",
        ),
        (
            TokenType::CommentSingle,
            "// hash function. This is intended to be called from the init function in\n",
        ),
        (
            TokenType::CommentSingle,
            "// packages that implement hash functions.\n",
        ),
        (TokenType::KeywordDeclaration, "func"),
        (TokenType::Text, " "),
        (TokenType::NameFunction, "RegisterHash"),
        (TokenType::Punctuation, "("),
        (TokenType::NameOther, "h"),
        (TokenType::Text, " "),
        (TokenType::NameOther, "Hash"),
        (TokenType::Punctuation, ","),
        (TokenType::Text, " "),
        (TokenType::NameOther, "f"),
        (TokenType::Text, " "),
        (TokenType::KeywordDeclaration, "func"),
        (TokenType::Punctuation, "("),
        (TokenType::Punctuation, ")"),
        (TokenType::Text, " "),
        (TokenType::NameOther, "hash"),
        (TokenType::Punctuation, "."),
        (TokenType::NameOther, "Hash"),
        (TokenType::Punctuation, ")"),
        (TokenType::Text, " "),
        (TokenType::Punctuation, "{"),
        (TokenType::Text, "\n"),
        (TokenType::Text, "\t"),
        (TokenType::Keyword, "if"),
        (TokenType::Text, " "),
        (TokenType::NameOther, "h"),
        (TokenType::Text, " "),
        (TokenType::Operator, ">="),
        (TokenType::Text, " "),
        (TokenType::NameOther, "maxHash"),
        (TokenType::Text, " "),
        (TokenType::Punctuation, "{"),
        (TokenType::Text, "\n"),
        (TokenType::Text, "\t\t"),
        (TokenType::NameBuiltin, "panic"),
        (TokenType::Punctuation, "("),
        (
            TokenType::LiteralString,
            "\"crypto: RegisterHash of unknown hash function\"",
        ),
        (TokenType::Punctuation, ")"),
        (TokenType::Text, "\n"),
        (TokenType::Text, "\t"),
        (TokenType::Punctuation, "}"),
        (TokenType::Text, "\n"),
        (TokenType::Text, "\t"),
        (TokenType::NameOther, "hashes"),
        (TokenType::Punctuation, "["),
        (TokenType::NameOther, "h"),
        (TokenType::Punctuation, "]"),
        (TokenType::Text, " "),
        (TokenType::Punctuation, "="),
        (TokenType::Text, " "),
        (TokenType::NameOther, "f"),
        (TokenType::Text, "\n"),
        (TokenType::Punctuation, "}"),
        (TokenType::Text, "\n"),
    ];

    assert_eq!(
        tokens.len(),
        expected.len(),
        "token count mismatch: got {}, expected {}",
        tokens.len(),
        expected.len()
    );

    for (i, (tok, (exp_type, exp_val))) in tokens.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            tok.token_type, *exp_type,
            "token[{}] type mismatch: got {:?}, expected {:?} (value={:?})",
            i, tok.token_type, exp_type, tok.value
        );
        assert_eq!(
            tok.value, *exp_val,
            "token[{}] value mismatch: got {:?}, expected {:?}",
            i, tok.value, exp_val
        );
    }
}

#[test]
fn test_go_lines_match_chroma() {
    let tokens = tokenize("go", CODE_SNIPPET).unwrap();
    let lines = split_into_lines(&tokens);

    // Expected: 9 lines (Go dumps show LINE[0] through LINE[8])
    assert_eq!(lines.len(), 9, "line count mismatch");

    // LINE[0]: 1 token — the first comment
    assert_eq!(lines[0].len(), 1);
    assert!(lines[0][0].value.starts_with("// RegisterHash"));

    // LINE[1]: 2 tokens — empty continuation + second comment
    assert_eq!(lines[1].len(), 2);
    assert_eq!(lines[1][0].value, "");
    assert!(lines[1][1].value.starts_with("// hash function"));

    // LINE[3]: 23 tokens
    assert_eq!(lines[3].len(), 23, "line[3] token count");

    // LINE[4]: 12 tokens
    assert_eq!(lines[4].len(), 12, "line[4] token count");
}
