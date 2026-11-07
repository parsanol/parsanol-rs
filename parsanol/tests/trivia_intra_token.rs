//! expressir#598 regression: the trivia injector must never place
//! trivia inside a token's spelling. Intra-keyword trivia made the
//! byte-range keyword `(S)(I)(N)` match `"s IN"` across the
//! whitespace — an identifier `s` before `IN` parsed as the keyword
//! SIN, the relation vanished, and `s IN t` failed outright while
//! `s IN (t)` became a function call. The same defect split
//! multi-character operators (`":" "<" ">" ":"` matched `": < > :"`).

use parsanol::parg::frontend;
use parsanol::portable::{AstArena, PortableParser};

const GRAMMAR: &str = r#"grammar T598 version "0.1.0" edition 2 {
  atomic space = (%x20 / %x0D / %x0A / %x09 / %x0C)
  atomic spaces = (0* space) as spaces
  trivia = 1* (%x20 / %x0D / %x0A / %x09 / %x0C)
  atomic tSIN = (spaces ((%x53 / %x73) (%x49 / %x69) (%x4E / %x6E) !((%x61-7A / %x41-5A / %x30-39 / %x5F))) as str) as tSIN
  atomic tLIKE = (spaces ((%x4C / %x6C) (%x49 / %x69) (%x4B / %x6B) (%x45 / %x65) !((%x61-7A / %x41-5A / %x30-39 / %x5F))) as str) as tLIKE
  atomic tIN = (spaces ((%x49 / %x69) (%x4E / %x6E) !((%x61-7A / %x41-5A / %x30-39 / %x5F))) as str) as tIN
  atomic opDECL = (spaces ((":" "=")) as str) as opDECL
  atomic opLTGT = (spaces ((":" "<" ">" ":")) as str) as opLTGT
  atomic simpleId = !((tSIN / tLIKE)) (spaces ((%x41-5A / %x61-7A / %x5F) 0* (%x30-39 / %x41-5A / %x61-7A / %x5F)) as str) as simpleId
  skip = trivia
  term = (tSIN / tLIKE / simpleId) as term
  relation = (term (0* (((tIN / tLIKE / opDECL / opLTGT) term) as item))) as relation
  entry syntax: relation
}
"#;

fn parses(input: &str) -> bool {
    let mut parsed = frontend::parser::parse_grammar(GRAMMAR).expect("grammar parses");
    let tables = frontend::compile::Tables::default();
    let mut compiler =
        frontend::compile::compiler_for(&parsed.document, &tables, &parsed.atomic_rules)
            .expect("grammar compiles");
    let grammar = compiler
        .compile_entry_grammar("relation")
        .expect("entry grammar compiles");
    let mut arena = AstArena::for_input(input.len());
    arena.set_input(input.to_string());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.parse().is_ok()
}

#[test]
fn keyword_does_not_match_across_whitespace() {
    // The discriminator: with intra-keyword trivia the keyword SIN
    // consumed `"s IN"` as the term, `t` dangled with no operator, and
    // the relation failed. The fixed compiler keeps the spelling
    // contiguous, so `s` stays an identifier and IN is the operator.
    assert!(parses("s IN t"), "s IN t must parse as a relation");
    assert!(!parses("s IN"), "s IN must not parse as the keyword SIN");
    assert!(parses("sin"), "the real keyword still matches");
    assert!(
        parses("sint"),
        "identifier with a keyword prefix stays an identifier"
    );
    assert!(parses("s"), "a plain identifier still parses");
    assert!(!parses("s i n t"), "spaced letters are not the keyword");
}

#[test]
fn operator_does_not_match_across_whitespace() {
    assert!(parses("a := b"), ":= matches contiguously");
    assert!(!parses("a : = b"), "spaced := is two tokens, not one");
    assert!(!parses("a : < > : b"), "spaced :<:> is not one token");
}
