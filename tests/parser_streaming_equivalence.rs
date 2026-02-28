//! Tests for ensuring parser produces consistent AST output
//!
//! These tests verify that the parser's `to_parslet_compatible` transformation
//! produces correct AST structures, especially for repetition patterns.

use parsanol::portable::arena::AstArena;
use parsanol::portable::ast::AstNode;
use parsanol::portable::parser::PortableParser;
use parsanol::portable::parslet_transform::to_parslet_compatible;
use parsanol::portable::parser_dsl::{GrammarBuilder, ParsletExt, re, str};

/// Helper to convert AST to debug string for comparison
fn ast_to_debug_string(node: &AstNode, arena: &AstArena, input: &str) -> String {
    match node {
        AstNode::Nil => "nil".to_string(),
        AstNode::Bool(b) => format!("bool({})", b),
        AstNode::Int(n) => format!("int({})", n),
        AstNode::Float(f) => format!("float({})", f),
        AstNode::StringRef { pool_index } => {
            let (s, _, _) = arena.get_string_parts(*pool_index as usize);
            format!("string({:?})", s)
        }
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = start + (*length as usize);
            let s = if end <= input.len() { &input[start..end] } else { "" };
            format!("input_ref({:?} @ {})", s, offset)
        }
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            let inner: Vec<String> = items.iter()
                .map(|item| ast_to_debug_string(item, arena, input))
                .collect();
            format!("[{}]", inner.join(", "))
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let inner: Vec<String> = pairs.iter()
                .map(|(k, v)| format!("{:?}: {}", k, ast_to_debug_string(v, arena, input)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
    }
}

/// Parse input and transform to Parslet-compatible format
fn parse_and_transform(input: &str, grammar: &parsanol::portable::grammar::Grammar) -> (AstNode, AstArena) {
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(grammar, input, &mut arena);
    let raw = parser.parse().unwrap();
    let transformed = to_parslet_compatible(&raw, &mut arena, input);
    (transformed, arena)
}

// ============================================================================
// Test Cases
// ============================================================================

#[test]
fn test_simple_string_match() {
    // Grammar: str("hello")
    let grammar = GrammarBuilder::new()
        .rule("test", str("hello"))
        .build();

    let input = "hello";
    let (result, arena) = parse_and_transform(input, &grammar);

    let debug = ast_to_debug_string(&result, &arena, input);
    assert!(debug.contains("hello"), "Result: {}", debug);
}

#[test]
fn test_named_capture() {
    // Grammar: str("hello").label("greeting")
    let grammar = GrammarBuilder::new()
        .rule("test", str("hello").label("greeting"))
        .build();

    let input = "hello";
    let (result, arena) = parse_and_transform(input, &grammar);

    let debug = ast_to_debug_string(&result, &arena, input);
    assert!(debug.contains("greeting"), "Result: {}", debug);

    // Should be a hash with single key "greeting"
    if let AstNode::Hash { pool_index, length } = result {
        let pairs = arena.get_hash_items(pool_index as usize, length as usize);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "greeting");
    } else {
        panic!("Expected hash, got: {:?}", result);
    }
}

#[test]
fn test_repetition_with_named_captures() {
    // Grammar: re("[a-z]").label("letter").repeat(1, None)
    // This is the critical test case from the AST mismatch issue
    let grammar = GrammarBuilder::new()
        .rule("letters", re("[a-z]").label("letter").repeat(1, None))
        .build();

    let input = "abc";
    let (result, arena) = parse_and_transform(input, &grammar);

    let debug = ast_to_debug_string(&result, &arena, input);
    println!("Repetition result: {}", debug);

    // Should be an array of 3 hashes: [{:letter => "a"}, {:letter => "b"}, {:letter => "c"}]
    if let AstNode::Array { pool_index, length } = result {
        let items = arena.get_array(pool_index as usize, length as usize);
        assert_eq!(items.len(), 3, "Expected 3 items, got {}", items.len());

        // Each item should be a hash with key "letter"
        for (i, item) in items.iter().enumerate() {
            if let AstNode::Hash { pool_index: h_p, length: h_l } = item {
                let pairs = arena.get_hash_items(*h_p as usize, *h_l as usize);
                assert_eq!(pairs.len(), 1, "Item {} should have 1 key", i);
                assert_eq!(pairs[0].0, "letter", "Item {} should have 'letter' key", i);
            } else {
                panic!("Item {} should be a hash, got: {:?}", i, item);
            }
        }
    } else {
        panic!("Expected array, got: {:?}", result);
    }
}

#[test]
fn test_repetition_pattern_simple() {
    // Test repetition pattern
    let grammar = GrammarBuilder::new()
        .rule("test", re("[a-z]").label("letter").repeat(1, None))
        .build();

    let input = "abc";
    let (result, arena) = parse_and_transform(input, &grammar);

    // Should be array (repetition pattern with simple values)
    match result {
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(pool_index as usize, length as usize);
            assert_eq!(items.len(), 3);
        }
        _ => panic!("Expected array for repetition pattern, got: {:?}", result),
    }
}

#[test]
fn test_empty_repetition() {
    // Grammar: re("[a-z]").label("letter").repeat(0, None) (0 or more)
    let grammar = GrammarBuilder::new()
        .rule("letters", re("[a-z]").label("letter").repeat(0, None))
        .build();

    let input = ""; // Empty input
    let (result, arena) = parse_and_transform(input, &grammar);

    let debug = ast_to_debug_string(&result, &arena, input);
    println!("Empty repetition result: {}", debug);

    // Should be an empty array or nil
    match result {
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(pool_index as usize, length as usize);
            assert_eq!(items.len(), 0, "Expected empty array");
        }
        AstNode::Nil => {
            // Nil is also acceptable for empty match
        }
        _ => panic!("Expected empty array or nil, got: {:?}", result),
    }
}

#[test]
fn test_digit_repetition() {
    // Grammar: re("[0-9]").repeat(1, None) - digits without labels
    let grammar = GrammarBuilder::new()
        .rule("number", re("[0-9]").repeat(1, None))
        .build();

    let input = "123";
    let (result, arena) = parse_and_transform(input, &grammar);

    let debug = ast_to_debug_string(&result, &arena, input);
    println!("Digit repetition result: {}", debug);

    // Without labels, this should be an array of input refs
    if let AstNode::Array { pool_index, length } = result {
        let items = arena.get_array(pool_index as usize, length as usize);
        assert_eq!(items.len(), 3, "Expected 3 items");
    }
}

#[test]
fn test_word_repetition() {
    // Grammar: re("[a-z]+").label("word").repeat(1, None)
    let grammar = GrammarBuilder::new()
        .rule("words", re("[a-z]+").label("word").repeat(1, None))
        .build();

    let input = "hello";
    let (result, arena) = parse_and_transform(input, &grammar);

    let debug = ast_to_debug_string(&result, &arena, input);
    println!("Word repetition result: {}", debug);

    // Should be an array with 1 item (the whole word)
    if let AstNode::Array { pool_index, length } = result {
        let items = arena.get_array(pool_index as usize, length as usize);
        assert_eq!(items.len(), 1, "Expected 1 item");

        // The item should be a hash with key "word"
        if let AstNode::Hash { pool_index: h_p, length: h_l } = &items[0] {
            let pairs = arena.get_hash_items(*h_p as usize, *h_l as usize);
            assert_eq!(pairs[0].0, "word");
        }
    }
}
