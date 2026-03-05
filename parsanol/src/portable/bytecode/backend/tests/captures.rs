//! Capture tests
//!
//! Tests verifying both position parity AND AST structure parity
//! for named captures between packrat and bytecode backends.

use super::*;
use crate::portable::ast::AstNode;
use crate::portable::bytecode::backend::Parser;

#[test]
fn test_backend_parity_simple_capture() {
    // Test simple named capture: letter:"a"
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let letter = grammar.add_atom(Atom::Named {
        name: "letter".to_string(),
        atom: a,
    });

    grammar.root = letter;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();

    // Position parity - both should match same input
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);

    // AST structure parity - both should produce Hash for named capture
    assert!(matches!(packrat_result.value, AstNode::Hash { .. }));
    assert!(matches!(bytecode_result.value, AstNode::Hash { .. }));
}

#[test]
fn test_backend_parity_sequence_with_captures() {
    // Test sequence with multiple captures: (first:"a" second:"b")
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    }); // 1

    let first = grammar.add_atom(Atom::Named {
        // 2
        name: "first".to_string(),
        atom: a,
    });
    let second = grammar.add_atom(Atom::Named {
        // 3
        name: "second".to_string(),
        atom: b,
    });

    let seq = grammar.add_atom(Atom::Sequence {
        // 4
        atoms: vec![first, second],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("ab").unwrap();
    let bytecode_result = bytecode_parser.parse("ab").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 2);

    // AST structure parity - both should produce Array of Hash nodes
    assert!(matches!(packrat_result.value, AstNode::Array { .. }));
    assert!(matches!(bytecode_result.value, AstNode::Array { .. }));
}

#[test]
fn test_backend_parity_nested_capture() {
    // Test nested capture: outer:(inner:"ab")
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    }); // 1
    let ab = grammar.add_atom(Atom::Sequence { atoms: vec![a, b] }); // 2

    let inner = grammar.add_atom(Atom::Named {
        // 3
        name: "inner".to_string(),
        atom: ab,
    });
    let outer = grammar.add_atom(Atom::Named {
        // 4
        name: "outer".to_string(),
        atom: inner,
    });

    grammar.root = outer;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("ab").unwrap();
    let bytecode_result = bytecode_parser.parse("ab").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 2);

    // AST structure parity - both should produce Hash nodes
    assert!(matches!(packrat_result.value, AstNode::Hash { .. }));
    assert!(matches!(bytecode_result.value, AstNode::Hash { .. }));
}

#[test]
fn test_backend_parity_capture_with_repetition() {
    // Test capture with repetition: letters:("a")+
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0
    let a_plus = grammar.add_atom(Atom::Repetition {
        // 1
        atom: a,
        min: 1,
        max: None,
    });
    let letters = grammar.add_atom(Atom::Named {
        // 2
        name: "letters".to_string(),
        atom: a_plus,
    });

    grammar.root = letters;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("aaa").unwrap();
    let bytecode_result = bytecode_parser.parse("aaa").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 3);

    // AST structure parity - both should produce Hash nodes
    assert!(matches!(packrat_result.value, AstNode::Hash { .. }));
    assert!(matches!(bytecode_result.value, AstNode::Hash { .. }));
}

#[test]
fn test_backend_parity_alternative_with_captures() {
    // Test alternative with captures: (a:"x" | b:"y")
    let mut grammar = Grammar::new();

    let x = grammar.add_atom(Atom::Str {
        pattern: "x".to_string(),
    }); // 0
    let y = grammar.add_atom(Atom::Str {
        pattern: "y".to_string(),
    }); // 1

    let a_cap = grammar.add_atom(Atom::Named {
        // 2
        name: "a".to_string(),
        atom: x,
    });
    let b_cap = grammar.add_atom(Atom::Named {
        // 3
        name: "b".to_string(),
        atom: y,
    });

    let alt = grammar.add_atom(Atom::Alternative {
        // 4
        atoms: vec![a_cap, b_cap],
    });

    grammar.root = alt;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Test first alternative
    let packrat_result = packrat_parser.parse("x").unwrap();
    let bytecode_result = bytecode_parser.parse("x").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
    // AST structure parity - both should produce Hash nodes
    assert!(matches!(packrat_result.value, AstNode::Hash { .. }));
    assert!(matches!(bytecode_result.value, AstNode::Hash { .. }));

    // Test second alternative
    let packrat_result = packrat_parser.parse("y").unwrap();
    let bytecode_result = bytecode_parser.parse("y").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
    // AST structure parity - both should produce Hash nodes
    assert!(matches!(packrat_result.value, AstNode::Hash { .. }));
    assert!(matches!(bytecode_result.value, AstNode::Hash { .. }));
}

#[test]
fn test_backend_parity_ignore() {
    // Test ignore (match but discard): "a" ~"b" "c"
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

    let ignore_b = grammar.add_atom(Atom::Ignore { atom: b }); // 3

    let seq = grammar.add_atom(Atom::Sequence {
        // 4
        atoms: vec![a, ignore_b, c],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("abc").unwrap();
    let bytecode_result = bytecode_parser.parse("abc").unwrap();

    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 3);
}

#[test]
fn test_backend_parity_cut() {
    // Test cut (atomic/commit): "a" >> "b"
    // Once "a" matches, we commit and can't backtrack
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    }); // 1

    let cut = grammar.add_atom(Atom::Cut); // 2

    let seq = grammar.add_atom(Atom::Sequence {
        // 3
        atoms: vec![a, cut, b],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Should match "ab"
    let packrat_result = packrat_parser.parse("ab").unwrap();
    let bytecode_result = bytecode_parser.parse("ab").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 2);
}

#[test]
fn test_backend_parity_exponential_safe() {
    // Test a pattern that could be exponential but is handled correctly
    // Pattern: "a"* "a"* - this is safe because both repetitions are greedy
    // and will consume all 'a's, leaving nothing for the second repetition
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0

    let first_rep = grammar.add_atom(Atom::Repetition {
        // 1
        atom: a,
        min: 0,
        max: None,
    });

    let second_rep = grammar.add_atom(Atom::Repetition {
        // 2
        atom: a,
        min: 0,
        max: None,
    });

    let seq = grammar.add_atom(Atom::Sequence {
        // 3
        atoms: vec![first_rep, second_rep],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Both should match "aaa" and end at position 3
    let packrat_result = packrat_parser.parse("aaa").unwrap();
    let bytecode_result = bytecode_parser.parse("aaa").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 3);
}

#[test]
fn test_backend_parity_nested_repetition() {
    // Test nested repetition: ("a"+)+
    // This tests that nested repetitions work correctly
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0

    let inner = grammar.add_atom(Atom::Repetition {
        // 1
        atom: a,
        min: 1,
        max: None,
    });

    let outer = grammar.add_atom(Atom::Repetition {
        // 2
        atom: inner,
        min: 1,
        max: None,
    });

    grammar.root = outer;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Should match "aaa"
    let packrat_result = packrat_parser.parse("aaa").unwrap();
    let bytecode_result = bytecode_parser.parse("aaa").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 3);
}

#[test]
fn test_backend_parity_choice_in_repetition() {
    // Test choice inside repetition: ("a" | "b")*
    // This tests that alternatives work correctly within repetitions
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    }); // 1

    let choice = grammar.add_atom(Atom::Alternative {
        // 2
        atoms: vec![a, b],
    });

    let rep = grammar.add_atom(Atom::Repetition {
        // 3
        atom: choice,
        min: 0,
        max: None,
    });

    grammar.root = rep;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Should match "abba"
    let packrat_result = packrat_parser.parse("abba").unwrap();
    let bytecode_result = bytecode_parser.parse("abba").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 4);
}

#[test]
fn test_backend_parity_complex_backtracking() {
    // Test that both backends correctly implement PEG ordered choice semantics.
    // In standard PEG, `("a" | "aa") "b"` on input "aab" should FAIL
    // because once "a" matches, we commit to it and don't backtrack to try "aa"
    // when the subsequent "b" fails.
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0
    let aa = grammar.add_atom(Atom::Str {
        pattern: "aa".to_string(),
    }); // 1
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    }); // 2

    let choice = grammar.add_atom(Atom::Alternative {
        // 3
        atoms: vec![a, aa],
    });

    let seq = grammar.add_atom(Atom::Sequence {
        // 4
        atoms: vec![choice, b],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Test what each backend does
    let packrat_result = packrat_parser.parse("aab");
    let bytecode_result = bytecode_parser.parse("aab");

    // Both backends should produce the SAME result (both fail in this case)
    // Standard PEG semantics: once "a" matches in ("a" | "aa"), we commit and
    // don't backtrack to try "aa" when "b" fails.
    match (&packrat_result, &bytecode_result) {
        (Ok(p), Ok(b)) => {
            assert_eq!(p.end_pos, b.end_pos, "mismatch for input: aab");
        }
        (Err(_), Err(_)) => {
            // Both failed - correct PEG behavior
        }
        _ => {
            panic!(
                "Backend parity mismatch for input: 'aab' - packrat: {:?}, bytecode: {:?}",
                packrat_result, bytecode_result
            );
        }
    }
}

#[test]
fn test_backend_parity_empty_match() {
    // Test that both backends handle empty matches consistently
    // Pattern: "a"* should match empty string
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    }); // 0

    let rep = grammar.add_atom(Atom::Repetition {
        // 1
        atom: a,
        min: 0,
        max: None,
    });

    grammar.root = rep;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Should match empty string
    let packrat_result = packrat_parser.parse("").unwrap();
    let bytecode_result = bytecode_parser.parse("").unwrap();
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 0);
}

// ============================================================================
// PEG Ordered Choice Semantics Tests
// ============================================================================

#[test]
fn test_peg_ordered_choice_no_backtrack() {
    // PEG ordered choice: once an alternative succeeds, we commit to it.
    // Grammar: ("a" | "aa") "b" on input "aab" should FAIL.
    // Because "a" matches at pos 0, we commit to it, then "b" fails at pos 1.
    // We should NOT backtrack to try "aa".
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let aa = grammar.add_atom(Atom::Str {
        pattern: "aa".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    let choice = grammar.add_atom(Atom::Alternative { atoms: vec![a, aa] });
    let seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![choice, b],
    });

    grammar.root = seq;

    // Both backends should fail
    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    assert!(
        packrat_parser.parse("aab").is_err(),
        "packrat should fail on 'aab'"
    );
    assert!(
        bytecode_parser.parse("aab").is_err(),
        "bytecode should fail on 'aab'"
    );

    // But should succeed on "ab" (first alternative + b)
    assert!(
        packrat_parser.parse("ab").is_ok(),
        "packrat should succeed on 'ab'"
    );
    assert!(
        bytecode_parser.parse("ab").is_ok(),
        "bytecode should succeed on 'ab'"
    );

    // And succeed on "aab" with just the alternative (no b)
    let mut grammar2 = Grammar::new();
    let a2 = grammar2.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let aa2 = grammar2.add_atom(Atom::Str {
        pattern: "aa".to_string(),
    });
    let choice2 = grammar2.add_atom(Atom::Alternative {
        atoms: vec![a2, aa2],
    });
    grammar2.root = choice2;

    let mut packrat2 = Parser::packrat(grammar2.clone());
    let mut bytecode2 = Parser::bytecode(grammar2);

    let p_result = packrat2.parse("aa").unwrap();
    let b_result = bytecode2.parse("aa").unwrap();
    assert_eq!(
        p_result.end_pos, 1,
        "packrat should match 'a' (first alt) on 'aa'"
    );
    assert_eq!(
        b_result.end_pos, 1,
        "bytecode should match 'a' (first alt) on 'aa'"
    );
}

#[test]
fn test_peg_ordered_choice_three_alternatives() {
    // Test with three alternatives: ("a" | "aa" | "aaa") "b"
    // On input "aaab", should FAIL because "a" matches first, then "b" fails at "aab"
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let aa = grammar.add_atom(Atom::Str {
        pattern: "aa".to_string(),
    });
    let aaa = grammar.add_atom(Atom::Str {
        pattern: "aaa".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });

    let choice = grammar.add_atom(Atom::Alternative {
        atoms: vec![a, aa, aaa],
    });
    let seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![choice, b],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // Should fail - first alternative "a" matches, then "b" fails
    assert!(packrat_parser.parse("aaab").is_err());
    assert!(bytecode_parser.parse("aaab").is_err());

    // Should succeed on "ab"
    assert!(packrat_parser.parse("ab").is_ok());
    assert!(bytecode_parser.parse("ab").is_ok());
}

#[test]
fn test_peg_ordered_choice_nested() {
    // Nested alternatives: (("a" | "aa") "b" | "c")
    // On input "aab", should FAIL because inner ("a" | "aa") "b" fails,
    // then outer alternative tries "c" which also fails.
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let aa = grammar.add_atom(Atom::Str {
        pattern: "aa".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    let c = grammar.add_atom(Atom::Str {
        pattern: "c".to_string(),
    });

    let inner_choice = grammar.add_atom(Atom::Alternative { atoms: vec![a, aa] });
    let inner_seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![inner_choice, b],
    });
    let outer_choice = grammar.add_atom(Atom::Alternative {
        atoms: vec![inner_seq, c],
    });

    grammar.root = outer_choice;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // "aab" should fail
    assert!(packrat_parser.parse("aab").is_err());
    assert!(bytecode_parser.parse("aab").is_err());

    // "ab" should succeed (inner choice "a" + "b")
    assert!(packrat_parser.parse("ab").is_ok());
    assert!(bytecode_parser.parse("ab").is_ok());

    // "c" should succeed (outer second alternative)
    assert!(packrat_parser.parse("c").is_ok());
    assert!(bytecode_parser.parse("c").is_ok());
}

#[test]
fn test_peg_ordered_choice_with_prefix() {
    // Prefix before choice: "x" ("a" | "aa") "b" on input "xaab"
    // Should FAIL: "x" matches, "a" matches, "b" fails at "aab"
    let mut grammar = Grammar::new();

    let x = grammar.add_atom(Atom::Str {
        pattern: "x".to_string(),
    });
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let aa = grammar.add_atom(Atom::Str {
        pattern: "aa".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });

    let choice = grammar.add_atom(Atom::Alternative { atoms: vec![a, aa] });
    let seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![x, choice, b],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    assert!(packrat_parser.parse("xaab").is_err());
    assert!(bytecode_parser.parse("xaab").is_err());

    assert!(packrat_parser.parse("xab").is_ok());
    assert!(bytecode_parser.parse("xab").is_ok());
}

#[test]
fn test_peg_first_alternative_wins() {
    // Test that first matching alternative wins, even if later one is "better"
    // ("ab" | "a") "c" on input "abc"
    // "ab" matches at pos 0-1, then "c" matches at pos 2 - SUCCESS
    // But with ("a" | "ab") "c" on input "abc":
    // "a" matches at pos 0, then "c" fails at pos 1 (input is "bc") - FAIL
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let ab = grammar.add_atom(Atom::Str {
        pattern: "ab".to_string(),
    });
    let c = grammar.add_atom(Atom::Str {
        pattern: "c".to_string(),
    });

    let choice = grammar.add_atom(Atom::Alternative { atoms: vec![a, ab] });
    let seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![choice, c],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    // "abc" should fail because "a" matches first, then "c" fails at "bc"
    assert!(packrat_parser.parse("abc").is_err());
    assert!(bytecode_parser.parse("abc").is_err());

    // "ac" should succeed
    assert!(packrat_parser.parse("ac").is_ok());
    assert!(bytecode_parser.parse("ac").is_ok());
}
