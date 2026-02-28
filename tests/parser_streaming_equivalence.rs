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

// ============================================================================
// Streaming Builder Equivalence Tests
// ============================================================================

/// Test that parse_with_builder walks the TRANSFORMED AST (after to_parslet_compatible)
/// This is a regression test for the streaming builder AST mismatch issue.
#[test]
fn test_streaming_builder_uses_transformed_ast() {
    use parsanol::portable::streaming_builder::DebugBuilder;

    // Grammar: re("[a-z]").label("letter").repeat(1, None)
    // This is the critical pattern from the AST mismatch issue
    let grammar = GrammarBuilder::new()
        .rule("letters", re("[a-z]").label("letter").repeat(1, None))
        .build();

    let input = "abc";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    // Parse with builder - the result IS the the events (parse_with_builder calls finish())
    let mut builder = DebugBuilder::new();
    let result = parser.parse_with_builder(&mut builder);
    assert!(result.is_ok(), "parse_with_builder should succeed");

    // The result IS the events vector (not builder.finish() which would be empty)
    let events = result.unwrap();

    // Debug: print all events
    println!("Events for repetition pattern:");
    for (i, event) in events.iter().enumerate() {
        println!("  {}: {}", i, event);
    }

    // The streaming builder should see events from the TRANSFORMED AST.
    // For repetition pattern with named captures, the transformed AST is:
    // [{:letter => "a"}, {:letter => "b"}, {:letter => "c"}]
    //
    // So we expect:
    // - array_start(Some(3))
    // - hash_start, hash_key("letter"), string("a"), hash_value("letter"), hash_end(1) for each item
    // - array_end(3)

    // Find array events
    let array_start_idx = events.iter().position(|e| e.starts_with("array_start"));
    let array_end_idx = events.iter().position(|e| e.starts_with("array_end"));

    assert!(array_start_idx.is_some(), "Should have array_start event, got: {:?}", events);
    assert!(array_end_idx.is_some(), "Should have array_end event");

    // Should have 3 hash_start events (one for each letter)
    let hash_starts: Vec<_> = events.iter().filter(|e| e.starts_with("hash_start")).collect();
    assert_eq!(hash_starts.len(), 3, "Should have 3 hash_start events for repetition pattern, got: {:?}", events);

    // Should have 3 hash_key("letter") events
    let letter_keys: Vec<_> = events.iter().filter(|e| e.contains("hash_key(letter)")).collect();
    assert_eq!(letter_keys.len(), 3, "Should have 3 hash_key(letter) events, got: {:?}", events);
}

/// Test that streaming builder matches regular parser output for sequence with named captures
#[test]
fn test_streaming_builder_sequence_with_names() {
    use parsanol::portable::streaming_builder::DebugBuilder;
    use parsanol::portable::parser_dsl::{seq, dynamic};

    // Grammar: str("hello ").label("greeting") >> re("[a-z]+").label("name")
    let grammar = GrammarBuilder::new()
        .rule("test", seq(vec![
            dynamic(str("hello ").label("greeting")),
            dynamic(re("[a-z]+").label("name")),
        ]))
        .build();

    let input = "hello world";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let mut builder = DebugBuilder::new();
    let result = parser.parse_with_builder(&mut builder);
    assert!(result.is_ok());

    // The result IS the events vector (not builder.finish() which would be empty)
    let events = result.unwrap();

    // Debug: print all events
    println!("Events for sequence pattern:");
    for (i, event) in events.iter().enumerate() {
        println!("  {}: {}", i, event);
    }

    // For this grammar with seq(), the transformed AST is an array of two hashes:
    // [{:greeting => "hello "}, {:name => "world"}]
    // This is because seq() creates an array, not a merged hash.
    //
    // So we expect:
    // - array_start(Some(2))
    // - hash for greeting
    // - hash for name
    // - array_end(2)

    // Should have array start and end
    let array_start_idx = events.iter().position(|e| e.starts_with("array_start"));
    let array_end_idx = events.iter().position(|e| e.starts_with("array_end"));
    assert!(array_start_idx.is_some(), "Should have array_start");
    assert!(array_end_idx.is_some(), "Should have array_end");

    // Should have 2 hash_starts (one for each item in the array)
    let hash_starts: Vec<_> = events.iter().filter(|e| e.starts_with("hash_start")).collect();
    assert_eq!(hash_starts.len(), 2, "Should have 2 hash_starts for array of 2 items, got: {:?}", events);

    // Should have both greeting and name keys
    let greeting_keys: Vec<_> = events.iter().filter(|e| e.contains("hash_key(greeting)")).collect();
    let name_keys: Vec<_> = events.iter().filter(|e| e.contains("hash_key(name)")).collect();
    assert_eq!(greeting_keys.len(), 1, "Should have 1 greeting key");
    assert_eq!(name_keys.len(), 1, "Should have 1 name key");
}
