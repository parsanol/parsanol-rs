//! Complex grammar tests
//!
//! Tests for complex grammars like JSON-like structures and arithmetic expressions.

use super::*;
use crate::portable::bytecode::backend::Parser;

#[test]
fn test_backend_parity_json_string() {
    // Test JSON string parsing
    let mut grammar = Grammar::new();

    let quote = grammar.add_atom(Atom::Str {
        pattern: "\"".to_string(),
    });
    let string_char = grammar.add_atom(Atom::Re {
        pattern: "[^\"]".to_string(),
    });
    let string_content = grammar.add_atom(Atom::Repetition {
        atom: string_char,
        min: 0,
        max: None,
    });
    let json_string = grammar.add_atom(Atom::Sequence {
        atoms: vec![quote, string_content, quote],
    });

    grammar.root = json_string;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let test_cases = vec![
        (r#""hello""#, 7),
        (r#""""#, 2), // empty string
        (r#""abc123""#, 8),
    ];

    for (input, expected_end) in test_cases {
        let packrat_result = packrat_parser.parse(input).unwrap();
        let bytecode_result = bytecode_parser.parse(input).unwrap();
        assert_eq!(
            packrat_result.end_pos, bytecode_result.end_pos,
            "mismatch for input: {}",
            input
        );
        assert_eq!(
            packrat_result.end_pos, expected_end,
            "wrong end pos for input: {}",
            input
        );
    }
}

#[test]
fn test_backend_parity_json_number() {
    // Test JSON number parsing
    let mut grammar = Grammar::new();

    let digit = grammar.add_atom(Atom::Re {
        pattern: "[0-9]".to_string(),
    });
    let number = grammar.add_atom(Atom::Repetition {
        atom: digit,
        min: 1,
        max: None,
    });

    grammar.root = number;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let test_cases = vec![("123", 3), ("0", 1), ("99999", 5)];

    for (input, expected_end) in test_cases {
        let packrat_result = packrat_parser.parse(input).unwrap();
        let bytecode_result = bytecode_parser.parse(input).unwrap();
        assert_eq!(
            packrat_result.end_pos, bytecode_result.end_pos,
            "mismatch for input: {}",
            input
        );
        assert_eq!(
            packrat_result.end_pos, expected_end,
            "wrong end pos for input: {}",
            input
        );
    }
}

#[test]
fn test_backend_parity_arithmetic() {
    let grammar = arithmetic_grammar();

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let test_cases = vec![
        ("1", 1),
        ("123", 3),
        ("1+2", 3),
        ("1 + 2", 5),
        ("1+2+3", 5),
        ("10 - 5 + 3", 10),
        ("  42  ", 6),
    ];

    for (input, expected_end) in test_cases {
        let packrat_result = packrat_parser.parse(input);
        let bytecode_result = bytecode_parser.parse(input);

        match (&packrat_result, &bytecode_result) {
            (Ok(p), Ok(b)) => {
                assert_eq!(p.end_pos, b.end_pos, "mismatch for input: {:?}", input);
                assert_eq!(
                    p.end_pos, expected_end,
                    "wrong end pos for input: {:?}",
                    input
                );
            }
            (Err(_), Err(_)) => {
                // Both failed - acceptable if input was invalid
                if expected_end > 0 {
                    panic!("Both backends failed for valid input: {:?}", input);
                }
            }
            _ => {
                panic!(
                    "Backend parity mismatch for input: {:?} - packrat: {:?}, bytecode: {:?}",
                    input, packrat_result, bytecode_result
                );
            }
        }
    }
}

#[test]
fn test_backend_parity_nested_alternatives() {
    // Test nested alternatives: (a | b) (c | d)
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    }); // 1
    let c = grammar.add_atom(Atom::Str {
        pattern: "c".to_string(),
    }); // 2
    let d = grammar.add_atom(Atom::Str {
        pattern: "d".to_string(),
    }); // 3

    let first = grammar.add_atom(Atom::Alternative { atoms: vec![a, b] }); // 4
    let second = grammar.add_atom(Atom::Alternative { atoms: vec![c, d] }); // 5
    let seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![first, second],
    }); // 6
    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // All combinations should work
    for first_char in ["a", "b"] {
        for second_char in ["c", "d"] {
            let input = format!("{}{}", first_char, second_char);
            let packrat_result = packrat_parser.parse(&input).unwrap();
            let bytecode_result = bytecode_parser.parse(&input).unwrap();
            assert_eq!(
                packrat_result.end_pos, bytecode_result.end_pos,
                "mismatch for input: {}",
                input
            );
            assert_eq!(
                packrat_result.end_pos, 2,
                "wrong end pos for input: {}",
                input
            );
        }
    }
}

#[test]
fn test_backend_parity_deeply_nested_repetition() {
    // Test: ((a)+)+
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let a_plus = grammar.add_atom(Atom::Repetition {
        atom: a,
        min: 1,
        max: None,
    });
    let outer_plus = grammar.add_atom(Atom::Repetition {
        atom: a_plus,
        min: 1,
        max: None,
    });

    grammar.root = outer_plus;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let test_cases = vec![("a", 1), ("aa", 2), ("aaa", 3)];

    for (input, expected_end) in test_cases {
        let packrat_result = packrat_parser.parse(input).unwrap();
        let bytecode_result = bytecode_parser.parse(input).unwrap();
        assert_eq!(
            packrat_result.end_pos, bytecode_result.end_pos,
            "mismatch for input: {}",
            input
        );
        assert_eq!(
            packrat_result.end_pos, expected_end,
            "wrong end pos for input: {}",
            input
        );
    }
}

#[test]
fn test_backend_parity_many_optional() {
    // Test that both backends handle patterns with many optional elements
    // Pattern: (a? a? a? a? a?) which can have many ways to match
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0
    let a_opt = grammar.add_atom(Atom::Repetition {
        // 1
        atom: a,
        min: 0,
        max: Some(1),
    });

    // Sequence of 5 optional 'a's
    grammar.add_atom(Atom::Sequence {
        // 2
        atoms: vec![a_opt, a_opt, a_opt, a_opt, a_opt],
    });
    grammar.root = 2;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // This should still work correctly
    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);

    // Empty string should also match (all optional)
    let packrat_result = packrat_parser.parse("").unwrap();
    let bytecode_result = bytecode_parser.parse("").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 0);
}

#[test]
fn test_backend_parity_complex_json_like() {
    // Test a JSON-like structure: '[' value (',' value)* ']'
    // where value is a simple string or number
    let mut grammar = Grammar::new();

    // Primitives
    let quote = grammar.add_atom(Atom::Str {
        pattern: "\"".to_string(),
    });
    let string_content = grammar.add_atom(Atom::Re {
        pattern: "[^\"]".to_string(),
    });
    let string_inner = grammar.add_atom(Atom::Repetition {
        atom: string_content,
        min: 0,
        max: None,
    });
    let string_value = grammar.add_atom(Atom::Sequence {
        atoms: vec![quote, string_inner, quote],
    });

    let digit = grammar.add_atom(Atom::Re {
        pattern: "[0-9]".to_string(),
    });
    let number_value = grammar.add_atom(Atom::Repetition {
        atom: digit,
        min: 1,
        max: None,
    });

    // Value = string | number
    let value = grammar.add_atom(Atom::Alternative {
        atoms: vec![string_value, number_value],
    });

    // Array elements
    let lbracket = grammar.add_atom(Atom::Str {
        pattern: "[".to_string(),
    });
    let rbracket = grammar.add_atom(Atom::Str {
        pattern: "]".to_string(),
    });
    let comma = grammar.add_atom(Atom::Str {
        pattern: ",".to_string(),
    });
    let comma_value = grammar.add_atom(Atom::Sequence {
        atoms: vec![comma, value],
    });
    let tail = grammar.add_atom(Atom::Repetition {
        atom: comma_value,
        min: 0,
        max: None,
    });

    // Array = '[' value (',' value)* ']'
    let array = grammar.add_atom(Atom::Sequence {
        atoms: vec![lbracket, value, tail, rbracket],
    });

    grammar.root = array;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let test_cases = vec![
        (r#"["hello"]"#, 9),
        (r#"[123]"#, 5),
        (r#"["a","b"]"#, 9),
        (r#"[1,2,3]"#, 7),
    ];

    for (input, expected_end) in test_cases {
        let packrat_result = packrat_parser.parse(input);
        let bytecode_result = bytecode_parser.parse(input);

        match (&packrat_result, &bytecode_result) {
            (Ok(p), Ok(b)) => {
                assert_eq!(p.end_pos, b.end_pos, "mismatch for input: {:?}", input);
                assert_eq!(
                    p.end_pos, expected_end,
                    "wrong end pos for input: {:?}",
                    input
                );
            }
            (Err(pe), Err(be)) => {
                // Both failed - this is a parity success, but indicate the issue
                panic!(
                    "Both backends failed for input: {:?}\npackrat: {:?}\nbytecode: {:?}",
                    input, pe, be
                );
            }
            _ => {
                panic!(
                    "Backend parity mismatch for input: {:?} - packrat: {:?}, bytecode: {:?}",
                    input, packrat_result, bytecode_result
                );
            }
        }
    }
}
