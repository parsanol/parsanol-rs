//! rs#298 slice 1: the lexer parity gate. Every fixture holds the
//! RUBY compiler's token stream for a corpus source (type/value/
//! character-offset, ws and comments dropped); the Rust lexer must
//! reproduce it token for token. Drift = a release blocker for both
//! compilers; regenerate the fixtures with
//! `ruby tools/dump_parg_tokens.rb <source> <fixture>` in the same PR
//! as any Ruby lexer change.

use parsanol::parg::frontend::{lex, TokenKind};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/token_parity")
}

#[test]
fn rust_lexer_matches_the_ruby_streams() {
    let dir = fixture_dir();
    assert!(dir.is_dir(), "token_parity fixtures missing");

    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixture dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    fixtures.sort();

    let mut compared = 0usize;
    for path in &fixtures {
        let raw = std::fs::read_to_string(path).expect("fixture readable");
        let fixture: Value = serde_json::from_str(&raw).expect("fixture json");
        let source = fixture["source"].as_str().expect("source");
        let want = fixture["tokens"].as_array().expect("tokens");

        let got = lex(source)
            .unwrap_or_else(|err| panic!("{}: rust lexer failed: {err}", path.display()));

        assert_eq!(
            got.len(),
            want.len(),
            "{}: token count differs (rust {}, ruby {})",
            path.display(),
            got.len(),
            want.len()
        );
        for (token, expected) in got.iter().zip(want) {
            let name = expected["type"].as_str().expect("type");
            assert_eq!(
                token.kind.as_ruby_name(),
                name,
                "{}: kind drift at offset {}",
                path.display(),
                token.offset
            );
            assert_eq!(
                token.value,
                expected["value"].as_str().expect("value"),
                "{}: value drift at offset {}",
                path.display(),
                token.offset
            );
            assert_eq!(
                token.offset,
                expected["offset"].as_u64().expect("offset") as usize,
                "{}: offset drift on {}",
                path.display(),
                name
            );
        }
        compared += 1;
    }
    assert!(compared >= 30, "parity corpus shrank: {compared} fixtures");
    eprintln!("lexer parity: {compared} sources, token-for-token identical");
}

#[test]
fn token_kinds_cover_the_ruby_names() {
    // The parity wire maps every kind; this pins the mapping itself so
    // a new TokenKind cannot silently emit "punct".
    let kinds = [
        (TokenKind::Newline, "newline"),
        (TokenKind::Doc, "doc"),
        (TokenKind::Hex, "hex"),
        (TokenKind::Category, "category"),
        (TokenKind::Istring, "istr"),
        (TokenKind::Sstring, "sstr"),
        (TokenKind::Str, "str"),
        (TokenKind::Arrow, "arrow"),
        (TokenKind::Num, "num"),
        (TokenKind::Ident, "ident"),
        (TokenKind::Punct, "punct"),
    ];
    for (kind, name) in kinds {
        assert_eq!(kind.as_ruby_name(), name);
    }
}
