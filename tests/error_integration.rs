//! Integration tests for error reporting
//!
//! These tests cover error creation, formatting, and rich error messages.

use parsanol::portable::error::{ErrorBuilder, ErrorSeverity, Span};

// ============================================================================
// Span Tests
// ============================================================================

#[test]
fn test_span_at() {
    let span = Span::at(10, 2, 5);
    assert_eq!(span.start.offset, 10);
    assert_eq!(span.end.offset, 10);
    assert_eq!(span.start.line, 2);
    assert_eq!(span.start.column, 5);
}

#[test]
fn test_span_range() {
    let span = Span::range(10, 15, 2, 5, 2, 10);
    assert_eq!(span.start.offset, 10);
    assert_eq!(span.end.offset, 15);
    assert_eq!(span.start.line, 2);
    assert_eq!(span.start.column, 5);
    assert_eq!(span.end.line, 2);
    assert_eq!(span.end.column, 10);
}

#[test]
fn test_span_merge() {
    let a = Span::at(10, 2, 5);
    let b = Span::at(20, 3, 10);
    let merged = a.merge(&b);
    assert_eq!(merged.start.offset, 10);
    assert_eq!(merged.end.offset, 20);
}

// ============================================================================
// Error Builder Tests
// ============================================================================

#[test]
fn test_error_builder_simple() {
    let error = ErrorBuilder::new("Something went wrong").build();

    assert_eq!(error.message, "Something went wrong");
    assert!(error.context.is_none());
    assert!(error.children.is_empty());
}

#[test]
fn test_error_builder_with_position() {
    let error = ErrorBuilder::new("Parse error").at(10, 2, 5).build();

    assert_eq!(error.span.start.offset, 10);
    assert_eq!(error.span.start.line, 2);
    assert_eq!(error.span.start.column, 5);
}

#[test]
fn test_error_builder_with_context() {
    let error = ErrorBuilder::new("Parse error")
        .context("expression")
        .build();

    assert_eq!(error.context, Some("expression".to_string()));
}

#[test]
fn test_error_builder_with_child() {
    let child = ErrorBuilder::new("Expected '+'").build();

    let parent = ErrorBuilder::new("Failed to parse expression")
        .child(child)
        .build();

    assert_eq!(parent.children.len(), 1);
    assert_eq!(parent.children[0].message, "Expected '+'");
}

#[test]
fn test_error_builder_nested() {
    let grandchild = ErrorBuilder::new("Unexpected end of input").build();

    let child = ErrorBuilder::new("Expected number")
        .child(grandchild)
        .build();

    let parent = ErrorBuilder::new("Failed to parse expression")
        .at(10, 2, 5)
        .context("binary operation")
        .child(child)
        .build();

    assert_eq!(parent.children.len(), 1);
    assert_eq!(parent.children[0].children.len(), 1);
    assert_eq!(
        parent.children[0].children[0].message,
        "Unexpected end of input"
    );
}

// ============================================================================
// Error Formatting Tests
// ============================================================================

#[test]
fn test_error_ascii_tree_simple() {
    let error = ErrorBuilder::new("Parse error").build();

    let tree = error.ascii_tree();
    assert!(tree.contains("Parse error"));
}

#[test]
fn test_error_ascii_tree_with_children() {
    let error = ErrorBuilder::new("Failed to parse")
        .child(ErrorBuilder::new("Expected 'a'").build())
        .child(ErrorBuilder::new("Expected 'b'").build())
        .build();

    let tree = error.ascii_tree();
    assert!(tree.contains("Failed to parse"));
    assert!(tree.contains("Expected 'a'"));
    assert!(tree.contains("Expected 'b'"));
}

#[test]
fn test_error_ascii_tree_with_location() {
    let error = ErrorBuilder::new("Error here").at(10, 3, 5).build();

    // ascii_tree() doesn't include location - use format_with_source
    let input = "hello world";
    let formatted = error.format_with_source(input);
    assert!(formatted.contains("line 3"));
    assert!(formatted.contains("column 5"));
}

#[test]
fn test_error_ascii_tree_with_context() {
    let error = ErrorBuilder::new("Error").context("number parsing").build();

    let tree = error.ascii_tree();
    assert!(tree.contains("number parsing"));
}

// ============================================================================
// Error Source Context Tests
// ============================================================================

#[test]
fn test_error_format_with_source() {
    let error = ErrorBuilder::new("Unexpected character")
        .at(5, 1, 6)
        .build();

    let input = "helloXworld";
    let formatted = error.format_with_source(input);

    assert!(formatted.contains("line 1"));
    assert!(formatted.contains("column 6"));
}

// ============================================================================
// Deep Error Tree Tests
// ============================================================================

#[test]
fn test_deep_error_tree() {
    let level3 = ErrorBuilder::new("Level 3 error").build();

    let level2 = ErrorBuilder::new("Level 2 error").child(level3).build();

    let level1 = ErrorBuilder::new("Level 1 error").child(level2).build();

    let root = ErrorBuilder::new("Root error").child(level1).build();

    let tree = root.ascii_tree();
    assert!(tree.contains("Root error"));
    assert!(tree.contains("Level 1 error"));
    assert!(tree.contains("Level 2 error"));
    assert!(tree.contains("Level 3 error"));
}

#[test]
fn test_wide_error_tree() {
    let mut builder = ErrorBuilder::new("Parent error");

    for i in 1..=5 {
        builder = builder.child(ErrorBuilder::new(format!("Child error {}", i)).build());
    }

    let error = builder.build();
    assert_eq!(error.children.len(), 5);

    let tree = error.ascii_tree();
    for i in 1..=5 {
        assert!(tree.contains(&format!("Child error {}", i)));
    }
}

// ============================================================================
// Error Message Formatting Tests
// ============================================================================

#[test]
fn test_error_display() {
    let error = ErrorBuilder::new("Test error message").at(0, 1, 1).build();

    let display = format!("{}", error);
    assert!(display.contains("Test error message"));
}

// ============================================================================
// Builder Chaining Tests
// ============================================================================

#[test]
fn test_builder_method_chaining() {
    let error = ErrorBuilder::new("Complex error")
        .at(100, 10, 5)
        .context("function call")
        .child(ErrorBuilder::new("Inner error 1").build())
        .child(ErrorBuilder::new("Inner error 2").build())
        .build();

    assert_eq!(error.span.start.offset, 100);
    assert!(error.context.is_some());
    assert_eq!(error.children.len(), 2);
}

// ============================================================================
// Error Severity Tests
// ============================================================================

#[test]
fn test_error_severity() {
    let error = ErrorBuilder::new("Warning message")
        .severity(ErrorSeverity::Warning)
        .build();

    assert_eq!(error.severity, ErrorSeverity::Warning);
}

// ============================================================================
// Deepest Position Tests
// ============================================================================

#[test]
fn test_deepest_position() {
    let parent = ErrorBuilder::new("Parent")
        .at(10, 2, 5)
        .child(ErrorBuilder::new("Child").at(50, 5, 10).build())
        .build();

    let deepest = parent.deepest_position();
    assert_eq!(deepest.start.offset, 50);
    assert_eq!(deepest.start.line, 5);
}
