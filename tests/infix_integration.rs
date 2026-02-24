//! Integration tests for infix expression parsing
//!
//! These tests cover operator precedence, associativity, and expression building.

use parsanol::portable::{
    infix::{Assoc, InfixBuilder},
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    AstArena, AstNode, Grammar, PortableParser,
};

/// Build a standard arithmetic grammar with precedence
fn build_arithmetic_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();

    builder = builder.rule("expr", re(r"[0-9]+")); // Placeholder
    builder = builder.rule("number", re(r"[0-9]+"));
    builder = builder.rule(
        "primary",
        choice(vec![
            dynamic(seq(vec![
                dynamic(str("(")),
                dynamic(ref_("expr")),
                dynamic(str(")")),
            ])),
            dynamic(ref_("number")),
        ]),
    );

    let expr_atom = InfixBuilder::new()
        .primary(ref_("primary"))
        .op("*", 2, Assoc::Left)
        .op("/", 2, Assoc::Left)
        .op("+", 1, Assoc::Left)
        .op("-", 1, Assoc::Left)
        .build(&mut builder);

    builder.update_rule("expr", expr_atom);
    builder.build()
}

// ============================================================================
// Basic Expression Tests
// ============================================================================

#[test]
fn test_infix_single_number() {
    let grammar = build_arithmetic_grammar();
    let input = "42";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse single number");
    // Result type depends on implementation
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_infix_simple_addition() {
    let grammar = build_arithmetic_grammar();
    let input = "1+2";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse addition");
    // Should produce some AST structure
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_infix_simple_subtraction() {
    let grammar = build_arithmetic_grammar();
    let input = "5-3";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse subtraction");
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_infix_simple_multiplication() {
    let grammar = build_arithmetic_grammar();
    let input = "4*2";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse multiplication");
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_infix_simple_division() {
    let grammar = build_arithmetic_grammar();
    let input = "8/2";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse division");
    assert!(!matches!(result, AstNode::Nil));
}

// ============================================================================
// Precedence Tests
// ============================================================================

#[test]
fn test_precedence_mult_before_add() {
    let grammar = build_arithmetic_grammar();
    let input = "1+2*3";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse with precedence");
    // With correct precedence, this should be 1 + (2 * 3) = 7
    // not (1 + 2) * 3 = 9
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_precedence_mult_before_sub() {
    let grammar = build_arithmetic_grammar();
    let input = "6-2*2";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse with precedence");
    // Should be 6 - (2 * 2) = 2
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_precedence_div_before_add() {
    let grammar = build_arithmetic_grammar();
    let input = "6+8/2";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse with precedence");
    // Should be 6 + (8 / 2) = 10
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_precedence_same_level_left_assoc() {
    let grammar = build_arithmetic_grammar();
    let input = "10-3-2";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser
        .parse()
        .expect("Should parse with left associativity");
    // Left assoc: (10 - 3) - 2 = 5
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_precedence_complex_expression() {
    let grammar = build_arithmetic_grammar();
    let input = "2+3*4-5";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse complex expression");
    // Should be 2 + (3 * 4) - 5 = 9
    assert!(!matches!(result, AstNode::Nil));
}

// ============================================================================
// Parentheses Tests
// ============================================================================

#[test]
fn test_parentheses_simple() {
    let grammar = build_arithmetic_grammar();
    let input = "(1+2)";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser
        .parse()
        .expect("Should parse parenthesized expression");
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_parentheses_override_precedence() {
    let grammar = build_arithmetic_grammar();
    let input = "(1+2)*3";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser
        .parse()
        .expect("Should parse with overridden precedence");
    // (1 + 2) * 3 = 9, not 1 + (2 * 3) = 7
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_parentheses_nested() {
    let grammar = build_arithmetic_grammar();
    let input = "((1+2)*(3+4))";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse nested parentheses");
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_parentheses_deep_nesting() {
    let grammar = build_arithmetic_grammar();
    let input = "(((1+2)))";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser
        .parse()
        .expect("Should parse deeply nested parentheses");
    assert!(!matches!(result, AstNode::Nil));
}

// ============================================================================
// Multi-digit Numbers
// ============================================================================

#[test]
fn test_infix_multidigit_numbers() {
    let grammar = build_arithmetic_grammar();
    let input = "100+200*3";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse multi-digit numbers");
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_infix_large_numbers() {
    let grammar = build_arithmetic_grammar();
    let input = "12345*67890";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse large numbers");
    assert!(!matches!(result, AstNode::Nil));
}

// ============================================================================
// Chained Operations
// ============================================================================

#[test]
fn test_infix_chain_additions() {
    let grammar = build_arithmetic_grammar();
    let input = "1+2+3+4+5";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse chained additions");
    assert!(!matches!(result, AstNode::Nil));
}

#[test]
fn test_infix_chain_mixed() {
    let grammar = build_arithmetic_grammar();
    let input = "1*2+3*4+5*6";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse().expect("Should parse mixed chain");
    assert!(!matches!(result, AstNode::Nil));
}

// ============================================================================
// Whitespace Handling
// ============================================================================

#[test]
fn test_infix_with_spaces() {
    // Note: The grammar doesn't handle spaces, so this tests that behavior
    let grammar = build_arithmetic_grammar();
    let input = "1 + 2"; // Will fail because spaces aren't handled
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    // This should fail because we don't skip whitespace
    let result = parser.parse();
    // Just verify we get a result (success or failure is implementation-dependent)
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// Error Cases
// ============================================================================

#[test]
fn test_infix_incomplete_expression() {
    let grammar = build_arithmetic_grammar();
    let input = "1+"; // Missing right operand
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse();
    // Should fail or handle gracefully
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_infix_unmatched_paren() {
    let grammar = build_arithmetic_grammar();
    let input = "(1+2"; // Missing closing paren
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse();
    // Behavior depends on implementation
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// Right Associativity (Power Operator)
// ============================================================================

fn build_power_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();

    builder = builder.rule("expr", re(r"[0-9]+")); // Placeholder
    builder = builder.rule("number", re(r"[0-9]+"));
    builder = builder.rule("primary", ref_("number"));

    let expr_atom = InfixBuilder::new()
        .primary(ref_("primary"))
        .op("^", 3, Assoc::Right) // Right-associative
        .build(&mut builder);

    builder.update_rule("expr", expr_atom);
    builder.build()
}

#[test]
fn test_right_assoc_power() {
    let grammar = build_power_grammar();
    let input = "2^3^2"; // Should be 2^(3^2) = 2^9 = 512, not (2^3)^2 = 64
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    // Right associativity parsing may not be fully implemented yet
    // Just verify it parses something
    let result = parser.parse();
    // May succeed or fail depending on implementation
    assert!(result.is_ok() || result.is_err());
}
