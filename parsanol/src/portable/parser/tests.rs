//! Parser Tests

use super::*;
use crate::portable::arena::AstArena;
use crate::portable::parser_dsl::{str, GrammarBuilder};

#[test]
fn test_parse_with_rich_error_success() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse_with_rich_error();
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_rich_error_failure() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "world";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse_with_rich_error();
    assert!(result.is_err());

    let error = result.unwrap_err();
    assert!(error.message.contains("Expected"));
}

#[test]
fn test_parse_with_trace_success() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let (result, trace) = parser.parse_with_trace();
    assert!(result.is_ok());
    assert!(!trace.entries.is_empty());
}

#[test]
fn test_parse_with_trace_failure() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "world";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let (result, trace) = parser.parse_with_trace();
    assert!(result.is_err());
    assert!(!trace.entries.is_empty());
}

#[test]
fn test_trace_format() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let (_, trace) = parser.parse_with_trace();
    let formatted = trace.format(&grammar);
    assert!(formatted.contains("Enter"));
}

#[test]
fn test_rich_error_format_with_source() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "world";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse_with_rich_error();
    if let Err(error) = result {
        let formatted = error.format_with_source("world");
        assert!(formatted.contains("line"));
        assert!(formatted.contains("column"));
    }
}

#[test]
fn test_set_timeout() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.set_timeout_ms(1000);

    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_set_max_memory() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.set_max_memory(1_000_000);

    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_memory_usage() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let parser = PortableParser::new(&grammar, input, &mut arena);

    let usage = parser.memory_usage();
    assert!(usage > 0);
}

#[test]
fn test_resource_limits_combined() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.set_timeout_ms(1000);
    parser.set_max_memory(1_000_000);
    parser.set_max_recursion_depth(100);

    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_builder() {
    use crate::portable::streaming_builder::DebugBuilder;

    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let mut builder = DebugBuilder::new();
    let result: Result<Vec<String>, _> = parser.parse_with_builder(&mut builder);

    assert!(result.is_ok());
    let events = result.unwrap();
    assert!(!events.is_empty());
}

#[test]
fn test_parse_with_builder_collects_strings() {
    use crate::portable::streaming_builder::BuilderStringCollector;

    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let mut builder = BuilderStringCollector::new();
    let result: Result<Vec<String>, _> = parser.parse_with_builder(&mut builder);

    assert!(result.is_ok());
    let strings = result.unwrap();
    assert_eq!(strings, vec!["hello"]);
}
