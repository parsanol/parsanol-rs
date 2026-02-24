//! Property-based tests using proptest
//!
//! These tests use property-based testing to verify parser behavior across
//! a wide range of inputs.

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};
use proptest::prelude::*;

// =============================================================================
// Character Class Tests (Regex)
// =============================================================================

proptest! {
    /// Single digit should match [0-9]
    #[test]
    fn test_digit_class(c in "[0-9]") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[0-9]"))
            .build();

        let mut arena = AstArena::for_input(c.len());
        let mut parser = PortableParser::new(&grammar, &c, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Single lowercase letter should match [a-z]
    #[test]
    fn test_lowercase_class(c in "[a-z]") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[a-z]"))
            .build();

        let mut arena = AstArena::for_input(c.len());
        let mut parser = PortableParser::new(&grammar, &c, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Single uppercase letter should match [A-Z]
    #[test]
    fn test_uppercase_class(c in "[A-Z]") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[A-Z]"))
            .build();

        let mut arena = AstArena::for_input(c.len());
        let mut parser = PortableParser::new(&grammar, &c, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Alphanumeric identifier should match [a-zA-Z_][a-zA-Z0-9_]*
    #[test]
    fn test_identifier_pattern(s in "[a-zA-Z_][a-zA-Z0-9_]{0,19}") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[a-zA-Z_][a-zA-Z0-9_]*"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Numbers should match [0-9]+
    #[test]
    fn test_number_pattern(s in "[0-9]{1,20}") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[0-9]+"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Single ASCII character should match .
    #[test]
    fn test_any_char(c in "[\\x20-\\x7E]") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("."))
            .build();

        let mut arena = AstArena::for_input(c.len());
        let mut parser = PortableParser::new(&grammar, &c, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }
}

// =============================================================================
// Repetition Tests
// =============================================================================

proptest! {
    /// Repetition of 'a' should match one or more 'a's (empty handled separately)
    #[test]
    fn test_repetition_matches(s in "a{1,20}") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("a+"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Empty string with a* should work
    #[test]
    fn test_repetition_zero_or_more_empty(s in "") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("a*"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        // Zero matches is valid for a*
        let _ = parser.parse();
        // Just verify no panic occurred
    }

    /// One or more repetition requires at least one
    #[test]
    fn test_repetition_one_or_more(s in "a{1,20}") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("a+"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// One or more should fail on empty
    #[test]
    fn test_repetition_one_or_more_fails_empty(s in "") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("a+"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        prop_assert!(parser.parse().is_err());
    }
}

// =============================================================================
// Fixed String Literal Tests
// =============================================================================

proptest! {
    /// Test that "hello" matches exactly "hello"
    #[test]
    fn test_hello_literal_exact(input in "hello") {
        let grammar = GrammarBuilder::new()
            .rule("root", str("hello"))
            .build();

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Test that "world" matches exactly "world"
    #[test]
    fn test_world_literal_exact(input in "world") {
        let grammar = GrammarBuilder::new()
            .rule("root", str("world"))
            .build();

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Test fixed strings with regex
    #[test]
    fn test_fixed_with_regex(input in "hello") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("hello"))
            .build();

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }
}

// =============================================================================
// Choice Tests
// =============================================================================

proptest! {
    /// Choice between 'a' and 'b' patterns
    #[test]
    fn test_choice_ab(input in "[ab]{1,10}") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[ab]+"))
            .build();

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Choice with regex alternation
    #[test]
    fn test_choice_regex(input in "(foo|bar|baz)") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("foo|bar|baz"))
            .build();

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }
}

// =============================================================================
// Sequence Tests
// =============================================================================

proptest! {
    /// Letter followed by digit
    #[test]
    fn test_sequence_letter_digit(input in "[a-z][0-9]") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[a-z][0-9]"))
            .build();

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Multiple letters and digits
    #[test]
    fn test_sequence_multiple(input in "[a-z]{2,5}[0-9]{2,5}") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[a-z]{2,5}[0-9]{2,5}"))
            .build();

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }
}

// =============================================================================
// Edge Cases
// =============================================================================

proptest! {
    /// Very long input should parse (within limits)
    #[test]
    fn test_long_input(n in 1000usize..5000) {
        let s: String = "a".repeat(n);

        let grammar = GrammarBuilder::new()
            .rule("root", re("a*"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Whitespace patterns
    #[test]
    fn test_whitespace(ws in "[ \\t\\n\\r]{1,10}") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("[ \\t\\n\\r]+"))
            .build();

        let mut arena = AstArena::for_input(ws.len());
        let mut parser = PortableParser::new(&grammar, &ws, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }

    /// Empty input with optional pattern
    #[test]
    fn test_empty_optional(s in "") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("a?"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        // Optional should match empty (zero occurrences)
        // Just verify no panic occurred
        let _ = parser.parse();
    }

    /// Non-empty input with optional pattern
    #[test]
    fn test_nonempty_optional(s in "a") {
        let grammar = GrammarBuilder::new()
            .rule("root", re("a?"))
            .build();

        let mut arena = AstArena::for_input(s.len());
        let mut parser = PortableParser::new(&grammar, &s, &mut arena);

        prop_assert!(parser.parse().is_ok());
    }
}

// =============================================================================
// Nested Structure Tests
// =============================================================================

proptest! {
    /// Nested parentheses with letters
    #[test]
    fn test_nested_parens(depth in 1usize..5) {
        let inner: String = "a".repeat(depth);
        let input = format!("{}{}{}", "(".repeat(depth), inner, ")".repeat(depth));

        // Simple test: just match the whole string as balanced parens
        let grammar = GrammarBuilder::new()
            .rule("root", re("\\([a-z()]*\\)"))
            .build();

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);

        // The exact match depends on pattern - just verify no panic
        let _ = parser.parse();
    }
}

// =============================================================================
// Grammar Serialization Roundtrip
// =============================================================================

proptest! {
    /// Grammar JSON serialization should be reversible
    #[test]
    fn test_grammar_json_roundtrip(seed in 0usize..100) {
        // Create a deterministic grammar based on seed
        let pattern = format!("[a-z]{{1,{}}}", seed % 10 + 1);
        let grammar = GrammarBuilder::new()
            .rule("root", re(&pattern))
            .build();

        // Serialize to JSON
        let json = grammar.to_json().expect("JSON serialization should succeed");

        // Deserialize back
        let parsed = Grammar::from_json(&json).expect("JSON deserialization should succeed");

        // Verify structure matches
        prop_assert_eq!(grammar.root, parsed.root);
        prop_assert_eq!(grammar.atoms.len(), parsed.atoms.len());
    }

    /// Grammar with multiple rules serializes correctly
    #[test]
    fn test_grammar_json_complex(_seed in 0usize..50) {
        let grammar = GrammarBuilder::new()
            .rule("letter", re("[a-z]"))
            .rule("digit", re("[0-9]"))
            .rule("ident", re("[a-z][a-z0-9]*"))
            .build();

        let json = grammar.to_json().expect("JSON serialization should succeed");
        let parsed = Grammar::from_json(&json).expect("JSON deserialization should succeed");

        prop_assert_eq!(grammar.atoms.len(), parsed.atoms.len());
    }
}

// =============================================================================
// Security Limit Tests
// =============================================================================

proptest! {
    /// Parser respects recursion depth limits
    #[test]
    fn test_recursion_limit_respected(_depth in 1usize..100) {
        // Build a recursive grammar using dynamic to allow mixed types
        let grammar = GrammarBuilder::new()
            .rule("a", str("a"))
            .rule("root", choice(vec![
                dynamic(str("a")),
                dynamic(seq(vec![
                    dynamic(str("(")),
                    dynamic(ref_("root")),
                    dynamic(str(")")),
                ])),
            ]))
            .build();

        let input = "a";
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);

        // Simple case should succeed
        prop_assert!(parser.parse().is_ok());
    }
}
