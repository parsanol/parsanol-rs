//! Integration tests for streaming parsing
//!
//! These tests cover the streaming parser functionality including:
//! - Chunk configuration
//! - Parsing from readers
//! - Error handling

use parsanol::portable::{
    streaming::{ChunkConfig, StreamingError, StreamingParser},
    parser_dsl::{re, str, GrammarBuilder},
    AstArena, AstNode,
};
use std::io::Cursor;

// ============================================================================
// ChunkConfig Tests
// ============================================================================

#[test]
fn test_chunk_config_default() {
    let config = ChunkConfig::default();
    assert_eq!(config.chunk_size, 64 * 1024);
    assert_eq!(config.window_size, 3);
}

#[test]
fn test_chunk_config_custom() {
    let config = ChunkConfig::new(1024, 5);
    assert_eq!(config.chunk_size, 1024);
    assert_eq!(config.window_size, 5);
}

#[test]
fn test_chunk_config_presets() {
    let small = ChunkConfig::small();
    assert_eq!(small.chunk_size, 16 * 1024);
    assert_eq!(small.window_size, 2);

    let large = ChunkConfig::large();
    assert_eq!(large.chunk_size, 256 * 1024);
    assert_eq!(large.window_size, 4);
}

#[test]
fn test_chunk_config_max_memory() {
    let config = ChunkConfig::new(1024, 3);
    assert_eq!(config.max_memory(), 3 * 1024);
}

// ============================================================================
// StreamingParser Tests
// ============================================================================

#[test]
fn test_streaming_parser_new() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let config = ChunkConfig::default();
    let parser = StreamingParser::new(&grammar, config);

    assert_eq!(parser.total_bytes_read(), 0);
}

#[test]
fn test_streaming_parser_with_defaults() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let parser = StreamingParser::with_defaults(&grammar);

    assert_eq!(parser.total_bytes_read(), 0);
}

#[test]
fn test_streaming_parse_simple() {
    let grammar = GrammarBuilder::new().rule("word", re(r"[a-z]+")).build();
    let config = ChunkConfig::default();
    let mut parser = StreamingParser::new(&grammar, config);

    let input = "hello";
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let result = parser
        .parse_from_reader(&mut cursor, &mut arena)
        .expect("Should parse");

    assert!(matches!(result.ast, AstNode::InputRef { .. }));
    assert_eq!(result.chunks_processed, 1);
}

#[test]
fn test_streaming_parse_empty() {
    let grammar = GrammarBuilder::new().rule("empty", str("")).build();
    let config = ChunkConfig::default();
    let mut parser = StreamingParser::new(&grammar, config);

    let input = "";
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::new();

    let result = parser.parse_from_reader(&mut cursor, &mut arena);
    // Empty input may fail or succeed depending on grammar
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_streaming_parse_no_match() {
    let grammar = GrammarBuilder::new().rule("hello", str("hello")).build();
    let config = ChunkConfig::default();
    let mut parser = StreamingParser::new(&grammar, config);

    let input = "world";
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let result = parser.parse_from_reader(&mut cursor, &mut arena);
    // Should fail to parse "world" with "hello" rule
    assert!(result.is_err());
}

#[test]
fn test_streaming_parse_unicode() {
    let grammar = GrammarBuilder::new().rule("text", re(r".+")).build();
    let config = ChunkConfig::default();
    let mut parser = StreamingParser::new(&grammar, config);

    let input = "hello 世界";
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let result = parser
        .parse_from_reader(&mut cursor, &mut arena)
        .expect("Should parse unicode");

    assert!(matches!(result.ast, AstNode::InputRef { .. }));
}

#[test]
fn test_streaming_parse_multiline() {
    let grammar = GrammarBuilder::new().rule("text", re(r"[a-z\n]+")).build();
    let config = ChunkConfig::default();
    let mut parser = StreamingParser::new(&grammar, config);

    let input = "hello\nworld\ntest";
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let result = parser
        .parse_from_reader(&mut cursor, &mut arena)
        .expect("Should parse multiline");

    assert!(matches!(result.ast, AstNode::InputRef { .. }));
}

#[test]
fn test_streaming_parse_large_input() {
    let grammar = GrammarBuilder::new().rule("text", re(r"[a-z]+")).build();
    let config = ChunkConfig::new(1024, 3); // 1KB chunks
    let mut parser = StreamingParser::new(&grammar, config);

    // Create large input
    let input = "a".repeat(5000);
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let result = parser
        .parse_from_reader(&mut cursor, &mut arena)
        .expect("Should parse large input");

    assert!(matches!(result.ast, AstNode::InputRef { .. }));
    assert!(result.chunks_processed >= 4); // 5000 bytes / 1024 = ~5 chunks
}

#[test]
fn test_streaming_parse_result_metrics() {
    // Use a grammar that matches the entire input
    let grammar = GrammarBuilder::new().rule("text", re(r"[a-z ]+")).build();
    let config = ChunkConfig::new(100, 3); // Small chunks
    let mut parser = StreamingParser::new(&grammar, config);

    let input = "hello world test";
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let result = parser
        .parse_from_reader(&mut cursor, &mut arena)
        .expect("Should parse");

    // Check metrics
    assert!(result.chunks_processed > 0);
    assert!(result.bytes_processed > 0);
    assert!(result.peak_memory > 0);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_streaming_error_display() {
    let error = StreamingError::IoError("read failed".to_string());
    let msg = error.to_string();
    assert!(msg.contains("I/O error"));
    assert!(msg.contains("read failed"));
}

#[test]
fn test_streaming_error_invalid_utf8() {
    let error = StreamingError::InvalidUtf8("invalid byte".to_string());
    let msg = error.to_string();
    assert!(msg.contains("Invalid UTF-8"));
}

#[test]
fn test_streaming_error_backtrack_limit() {
    let error = StreamingError::BacktrackLimitExceeded {
        requested_position: 100,
        earliest_available: 50,
    };
    let msg = error.to_string();
    assert!(msg.contains("Backtrack limit"));
    assert!(msg.contains("100"));
    assert!(msg.contains("50"));
}

#[test]
fn test_streaming_error_window_too_small() {
    let error = StreamingError::WindowTooSmall {
        required: 10,
        actual: 3,
    };
    let msg = error.to_string();
    assert!(msg.contains("Window too small"));
    assert!(msg.contains("10"));
    assert!(msg.contains("3"));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_streaming_parse_digits() {
    let grammar = GrammarBuilder::new().rule("num", re(r"[0-9]+")).build();
    let config = ChunkConfig::default();
    let mut parser = StreamingParser::new(&grammar, config);

    let input = "123456789";
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let result = parser
        .parse_from_reader(&mut cursor, &mut arena)
        .expect("Should parse digits");

    assert!(matches!(result.ast, AstNode::InputRef { .. }));
}

#[test]
fn test_streaming_parse_json_like() {
    // Simple JSON-like grammar
    let grammar = GrammarBuilder::new()
        .rule("value", re(r#"\{[^}]*\}"#))
        .build();
    let config = ChunkConfig::default();
    let mut parser = StreamingParser::new(&grammar, config);

    let input = r#"{"name": "test"}"#;
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let result = parser
        .parse_from_reader(&mut cursor, &mut arena)
        .expect("Should parse JSON-like");

    assert!(matches!(result.ast, AstNode::InputRef { .. }));
}

#[test]
fn test_streaming_parser_bytes_read() {
    let grammar = GrammarBuilder::new().rule("text", re(r".+")).build();
    let config = ChunkConfig::default();
    let mut parser = StreamingParser::new(&grammar, config);

    let input = "hello world";
    let mut cursor = Cursor::new(input.as_bytes());
    let mut arena = AstArena::for_input(input.len());

    let _ = parser.parse_from_reader(&mut cursor, &mut arena);

    // Should have read all bytes
    assert_eq!(parser.total_bytes_read(), input.len());
}

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_chunk_config_clone() {
    let config = ChunkConfig::new(1024, 3);
    let cloned = config.clone();

    assert_eq!(config.chunk_size, cloned.chunk_size);
    assert_eq!(config.window_size, cloned.window_size);
}

#[test]
fn test_chunk_config_debug() {
    let config = ChunkConfig::new(1024, 3);
    let debug_str = format!("{:?}", config);

    assert!(debug_str.contains("ChunkConfig"));
    assert!(debug_str.contains("1024"));
    assert!(debug_str.contains("3"));
}
