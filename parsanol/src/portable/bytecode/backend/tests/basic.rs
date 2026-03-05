//! Basic backend tests
//!
//! Tests for backend selection, characteristics, and simple parsing.

use super::*;
use crate::portable::bytecode::backend::{Backend, GrammarAnalysis, Parser};

#[test]
fn test_backend_selection() {
    let grammar = simple_grammar();

    let parser = Parser::packrat(grammar.clone());
    assert_eq!(parser.backend(), Backend::Packrat);

    let parser = Parser::bytecode(grammar.clone());
    assert_eq!(parser.backend(), Backend::Bytecode);

    let parser = Parser::auto(grammar);
    assert_eq!(parser.backend(), Backend::Auto);
}

#[test]
fn test_backend_is_methods() {
    assert!(Backend::Packrat.is_packrat());
    assert!(!Backend::Packrat.is_bytecode());

    assert!(Backend::Bytecode.is_bytecode());
    assert!(!Backend::Bytecode.is_packrat());

    assert!(Backend::Auto.is_auto());
    assert!(!Backend::Auto.is_bytecode());
}

#[test]
fn test_backend_names() {
    assert_eq!(Backend::Packrat.name(), "packrat");
    assert_eq!(Backend::Bytecode.name(), "bytecode");
    assert_eq!(Backend::Auto.name(), "auto");
}

#[test]
fn test_grammar_analysis_simple() {
    let grammar = simple_grammar();
    let analysis = GrammarAnalysis::analyze(&grammar);

    assert_eq!(analysis.atom_count, 1);
    assert!(!analysis.has_nested_repetition);
}

#[test]
fn test_grammar_analysis_recommended_backend() {
    let grammar = simple_grammar();
    let analysis = GrammarAnalysis::analyze(&grammar);

    // Simple grammar should recommend bytecode
    assert_eq!(analysis.recommended_backend(), Backend::Bytecode);
}

#[test]
fn test_parse_with_packrat() {
    let grammar = simple_grammar();
    let mut parser = Parser::packrat(grammar);
    let result = parser.parse("hello").unwrap();
    assert_eq!(result.end_pos, 5);
}

#[test]
fn test_parse_with_bytecode() {
    let grammar = simple_grammar();
    let mut parser = Parser::bytecode(grammar);
    let result = parser.parse("hello").unwrap();
    assert_eq!(result.end_pos, 5);
}

#[test]
fn test_parse_with_auto() {
    let grammar = simple_grammar();
    let mut parser = Parser::auto(grammar);

    // Should select bytecode for simple grammar
    assert_eq!(parser.effective_backend(), Backend::Bytecode);

    let result = parser.parse("hello").unwrap();
    assert_eq!(result.end_pos, 5);
}

#[test]
fn test_backend_parity_simple() {
    let grammar = simple_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("hello").unwrap();
    let bytecode_result = bytecode_parser.parse("hello").unwrap();

    // Both backends should produce same end position
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
}

#[test]
fn test_backend_parity_sequence() {
    let grammar = sequence_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("ab").unwrap();
    let bytecode_result = bytecode_parser.parse("ab").unwrap();

    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 2);
}

#[test]
fn test_backend_parity_alternative() {
    let grammar = alternative_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Test first alternative
    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);

    // Test second alternative
    let packrat_result = packrat_parser.parse("b").unwrap();
    let bytecode_result = bytecode_parser.parse("b").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
}

#[test]
fn test_backend_parity_repetition() {
    let grammar = repetition_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Test with multiple repetitions
    let packrat_result = packrat_parser.parse("aaa").unwrap();
    let bytecode_result = bytecode_parser.parse("aaa").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 3);
}

#[test]
fn test_backend_parity_regex() {
    let grammar = regex_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("123").unwrap();
    let bytecode_result = bytecode_parser.parse("123").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 3);
}

#[test]
fn test_backend_parity_positive_lookahead() {
    let grammar = lookahead_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Pattern: "a" &"b" "b" - match "a", check "b" ahead, match "b"
    // Should match "ab" fully
    let packrat_result = packrat_parser.parse("ab");
    let bytecode_result = bytecode_parser.parse("ab");

    // Both should succeed
    assert!(packrat_result.is_ok(), "packrat should succeed");
    assert!(bytecode_result.is_ok(), "bytecode should succeed");

    if let (Ok(p), Ok(b)) = (packrat_result, bytecode_result) {
        // Should match both characters
        assert_eq!(p.end_pos, b.end_pos);
        assert_eq!(p.end_pos, 2);
    }
}

#[test]
fn test_backend_parity_negative_lookahead() {
    let grammar = negative_lookahead_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Should match "b" when input does NOT start with "a"
    let packrat_result = packrat_parser.parse("b");
    let bytecode_result = bytecode_parser.parse("b");

    assert!(packrat_result.is_ok());
    assert!(bytecode_result.is_ok());

    if let (Ok(p), Ok(b)) = (packrat_result, bytecode_result) {
        assert_eq!(p.end_pos, b.end_pos);
        assert_eq!(p.end_pos, 1);
    }

    // Should fail when input starts with "a"
    let packrat_result = packrat_parser.parse("ab");
    let bytecode_result = bytecode_parser.parse("ab");

    assert!(packrat_result.is_err());
    assert!(bytecode_result.is_err());
}

#[test]
fn test_backend_parity_optional() {
    let grammar = optional_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // With "a": "ab" should match fully
    let packrat_result = packrat_parser.parse("ab").unwrap();
    let bytecode_result = bytecode_parser.parse("ab").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 2);

    // Without "a": "b" should match
    let packrat_result = packrat_parser.parse("b").unwrap();
    let bytecode_result = bytecode_parser.parse("b").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
}

#[test]
fn test_backend_parity_one_or_more() {
    let grammar = one_or_more_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Should match one or more "a"s
    let packrat_result = packrat_parser.parse("aaa").unwrap();
    let bytecode_result = bytecode_parser.parse("aaa").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 3);

    // Should match single "a"
    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);

    // Should fail on empty
    let packrat_result = packrat_parser.parse("");
    let bytecode_result = bytecode_parser.parse("");
    assert!(packrat_result.is_err());
    assert!(bytecode_result.is_err());
}

#[test]
fn test_backend_parity_three_alternatives() {
    // Test with 3 alternatives to verify choice patching works for more than 2
    let mut grammar = Grammar::new();
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    let c = grammar.add_atom(Atom::Str {
        pattern: "c".to_string(),
    });
    grammar.add_atom(Atom::Alternative { atoms: vec![a, b, c] });
    grammar.root = 3;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Test each alternative
    for input in ["a", "b", "c"].iter() {
        let packrat_result = packrat_parser.parse(input).unwrap();
        let bytecode_result = bytecode_parser.parse(input).unwrap();
        assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
        assert_eq!(packrat_result.end_pos, 1);
    }
}

#[test]
fn test_backend_parity_complex_sequence() {
    // Complex pattern: "a" "b"+ "c"?
    let mut grammar = Grammar::new();
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    let c = grammar.add_atom(Atom::Str {
        pattern: "c".to_string(),
    });
    let b_plus = grammar.add_atom(Atom::Repetition {
        atom: b,
        min: 1,
        max: None,
    });
    let c_opt = grammar.add_atom(Atom::Repetition {
        atom: c,
        min: 0,
        max: Some(1),
    });
    grammar.add_atom(Atom::Sequence {
        atoms: vec![a, b_plus, c_opt],
    });
    grammar.root = 5;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Test various inputs
    let test_cases = vec![
        ("abb", 3),  // a, bb, no c
        ("abbbc", 5), // a, bbb, c
        ("abc", 3),  // a, b, c
        ("ab", 2),   // a, b, no c
    ];

    for (input, expected_end) in test_cases {
        let packrat_result = packrat_parser.parse(input).unwrap();
        let bytecode_result = bytecode_parser.parse(input).unwrap();
        assert_eq!(packrat_result.end_pos, bytecode_result.end_pos, "mismatch for input: {}", input);
        assert_eq!(packrat_result.end_pos, expected_end, "wrong end pos for input: {}", input);
    }
}
