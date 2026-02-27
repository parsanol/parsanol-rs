//! Portable AST types for Parsanol
//!
//! This module defines AST nodes that can be allocated in an arena
//! without any Ruby or external dependencies. This enables the parser
//! to be compiled to WASM for Opal.

use std::fmt;

// Re-export SourcePosition from source_location for backward compatibility
pub use super::source_location::SourcePosition;

/// AST node types - optimized for arena allocation
///
/// Nodes store references to arena-allocated data using indices.
#[derive(Debug, Clone, Copy)]
pub enum AstNode {
    /// Nil/null value
    Nil,

    /// Boolean value
    Bool(bool),

    /// Integer value
    Int(i64),

    /// Floating point value
    Float(f64),

    /// Reference to interned string in arena
    ///
    /// The string data is stored in the arena's string pool.
    StringRef {
        /// Index into arena's string pool
        pool_index: u32,
    },

    /// Reference to original input string (zero-copy)
    ///
    /// Instead of copying the matched text, we store an offset
    /// and length into the original input. This is the most
    /// efficient representation for parse results.
    InputRef {
        /// Offset from start of input
        offset: u32,
        /// Length in bytes
        length: u32,
    },

    /// Array of child nodes (index into arena's array pool)
    Array {
        /// Index into arena's array pool
        pool_index: u32,
        /// Number of items
        length: u32,
    },

    /// Hash map (named captures)
    Hash {
        /// Index into arena's hash pool
        pool_index: u32,
        /// Number of entries
        length: u32,
    },
}

// Manual PartialEq implementation (f64 doesn't impl Eq)
impl PartialEq for AstNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AstNode::Nil, AstNode::Nil) => true,
            (AstNode::Bool(a), AstNode::Bool(b)) => a == b,
            (AstNode::Int(a), AstNode::Int(b)) => a == b,
            (AstNode::Float(a), AstNode::Float(b)) => a == b, // Compare bits for equality
            (AstNode::StringRef { pool_index: a }, AstNode::StringRef { pool_index: b }) => a == b,
            (
                AstNode::InputRef {
                    offset: o1,
                    length: l1,
                },
                AstNode::InputRef {
                    offset: o2,
                    length: l2,
                },
            ) => o1 == o2 && l1 == l2,
            (
                AstNode::Array {
                    pool_index: p1,
                    length: l1,
                },
                AstNode::Array {
                    pool_index: p2,
                    length: l2,
                },
            ) => p1 == p2 && l1 == l2,
            (
                AstNode::Hash {
                    pool_index: p1,
                    length: l1,
                },
                AstNode::Hash {
                    pool_index: p2,
                    length: l2,
                },
            ) => p1 == p2 && l1 == l2,
            _ => false,
        }
    }
}

/// Result of a parse operation
#[derive(Debug, Clone, Copy)]
pub struct ParseResult {
    /// The parsed AST node
    pub value: AstNode,
    /// Position after the matched content
    pub end_pos: usize,
}

/// Error type for parse operations
#[derive(Debug, Clone)]
pub enum ParseError {
    /// Expected something but found nothing
    Failed {
        /// The byte offset where parsing failed
        position: usize,
    },

    /// Parse didn't consume entire input
    Incomplete {
        /// Expected number of bytes to consume
        expected: usize,
        /// Actual number of bytes consumed
        actual: usize,
    },

    /// Invalid grammar specification
    InvalidGrammar {
        /// Reason why the grammar is invalid
        reason: String,
    },

    /// Internal error (shouldn't happen in normal use)
    Internal {
        /// Error message describing the internal error
        message: String,
    },

    /// Input exceeds maximum allowed size
    InputTooLarge {
        /// Size of the input in bytes
        input_size: usize,
        /// Maximum allowed size
        max_size: usize,
    },

    /// Recursion depth limit exceeded
    RecursionLimitExceeded {
        /// Current recursion depth
        depth: usize,
        /// Maximum allowed depth
        max_depth: usize,
    },

    /// Timeout exceeded during parsing
    TimeoutExceeded {
        /// Time elapsed in milliseconds
        elapsed_ms: u64,
        /// Timeout limit in milliseconds
        timeout_ms: u64,
    },

    /// Memory limit exceeded during parsing
    MemoryLimitExceeded {
        /// Memory used in bytes
        used_bytes: usize,
        /// Memory limit in bytes
        max_bytes: usize,
    },

    /// Error from streaming builder
    BuilderError {
        /// Error message from builder
        message: String,
    },
}

impl ParseError {
    /// Create a new Failed error
    #[inline]
    pub fn at_position(position: usize) -> Self {
        ParseError::Failed { position }
    }

    /// Add source position information to error message
    pub fn format_with_position(&self, input: &str) -> String {
        match self {
            ParseError::Failed { position } => {
                let sp = offset_to_position(input, *position);
                format!(
                    "Parse failed at line {}, column {} (byte offset {})",
                    sp.line, sp.column, position
                )
            }
            ParseError::Incomplete { expected, actual } => {
                format!(
                    "Parse incomplete: expected {} bytes, parsed {}",
                    expected, actual
                )
            }
            ParseError::InvalidGrammar { reason } => {
                format!("Invalid grammar: {}", reason)
            }
            ParseError::Internal { message } => {
                format!("Internal error: {}", message)
            }
            ParseError::InputTooLarge {
                input_size,
                max_size,
            } => {
                format!(
                    "Input too large: {} bytes exceeds limit of {} bytes",
                    input_size, max_size
                )
            }
            ParseError::RecursionLimitExceeded { depth, max_depth } => {
                format!(
                    "Recursion limit exceeded: depth {} exceeds limit of {}",
                    depth, max_depth
                )
            }
            ParseError::TimeoutExceeded {
                elapsed_ms,
                timeout_ms,
            } => {
                format!(
                    "Timeout exceeded: {}ms exceeds limit of {}ms",
                    elapsed_ms, timeout_ms
                )
            }
            ParseError::MemoryLimitExceeded {
                used_bytes,
                max_bytes,
            } => {
                format!(
                    "Memory limit exceeded: {} bytes exceeds limit of {} bytes",
                    used_bytes, max_bytes
                )
            }
            ParseError::BuilderError { message } => {
                format!("Builder error: {}", message)
            }
        }
    }
}

/// Convert byte offset to line/column position
/// Uses the canonical implementation from source_location module
#[inline]
pub fn offset_to_position(input: &str, offset: usize) -> SourcePosition {
    SourcePosition::from_offset(input, offset)
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Failed { position } => {
                write!(f, "Parse failed at position {}", position)
            }
            ParseError::Incomplete { expected, actual } => {
                write!(
                    f,
                    "Parse incomplete: expected {} bytes, parsed {}",
                    expected, actual
                )
            }
            ParseError::InvalidGrammar { reason } => {
                write!(f, "Invalid grammar: {}", reason)
            }
            ParseError::Internal { message } => {
                write!(f, "Internal error: {}", message)
            }
            ParseError::InputTooLarge {
                input_size,
                max_size,
            } => {
                write!(
                    f,
                    "Input too large: {} bytes exceeds limit of {} bytes",
                    input_size, max_size
                )
            }
            ParseError::RecursionLimitExceeded { depth, max_depth } => {
                write!(
                    f,
                    "Recursion limit exceeded: depth {} exceeds limit of {}",
                    depth, max_depth
                )
            }
            ParseError::TimeoutExceeded {
                elapsed_ms,
                timeout_ms,
            } => {
                write!(
                    f,
                    "Timeout exceeded: {}ms exceeds limit of {}ms",
                    elapsed_ms, timeout_ms
                )
            }
            ParseError::MemoryLimitExceeded {
                used_bytes,
                max_bytes,
            } => {
                write!(
                    f,
                    "Memory limit exceeded: {} bytes exceeds limit of {} bytes",
                    used_bytes, max_bytes
                )
            }
            ParseError::BuilderError { message } => {
                write!(f, "Builder error: {}", message)
            }
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    // === AstNode Tests ===

    #[test]
    fn test_ast_node_nil() {
        let node = AstNode::Nil;
        assert_eq!(node, AstNode::Nil);
    }

    #[test]
    fn test_ast_node_bool() {
        let node = AstNode::Bool(true);
        assert_eq!(node, AstNode::Bool(true));
        assert_ne!(node, AstNode::Bool(false));
    }

    #[test]
    fn test_ast_node_int() {
        let node = AstNode::Int(42);
        assert_eq!(node, AstNode::Int(42));
        assert_ne!(node, AstNode::Int(0));
    }

    #[test]
    fn test_ast_node_float() {
        let node = AstNode::Float(1.5);
        assert_eq!(node, AstNode::Float(1.5));
        assert_ne!(node, AstNode::Float(0.0));
    }

    #[test]
    fn test_ast_node_string_ref() {
        let node = AstNode::StringRef { pool_index: 5 };
        assert_eq!(node, AstNode::StringRef { pool_index: 5 });
        assert_ne!(node, AstNode::StringRef { pool_index: 0 });
    }

    #[test]
    fn test_ast_node_input_ref() {
        let node = AstNode::InputRef {
            offset: 10,
            length: 5,
        };
        assert_eq!(
            node,
            AstNode::InputRef {
                offset: 10,
                length: 5
            }
        );
    }

    #[test]
    fn test_ast_node_array() {
        let node = AstNode::Array {
            pool_index: 3,
            length: 5,
        };
        assert_eq!(
            node,
            AstNode::Array {
                pool_index: 3,
                length: 5
            }
        );
    }

    #[test]
    fn test_ast_node_hash() {
        let node = AstNode::Hash {
            pool_index: 7,
            length: 2,
        };
        assert_eq!(
            node,
            AstNode::Hash {
                pool_index: 7,
                length: 2
            }
        );
    }

    #[test]
    fn test_ast_node_different_variants_not_equal() {
        assert_ne!(AstNode::Nil, AstNode::Bool(false));
        assert_ne!(AstNode::Int(0), AstNode::Float(0.0));
        assert_ne!(
            AstNode::StringRef { pool_index: 0 },
            AstNode::InputRef {
                offset: 0,
                length: 0
            }
        );
    }

    // === ParseResult Tests ===

    #[test]
    fn test_parse_result() {
        let result = ParseResult {
            value: AstNode::Int(42),
            end_pos: 10,
        };
        assert_eq!(result.value, AstNode::Int(42));
        assert_eq!(result.end_pos, 10);
    }

    // === ParseError Tests ===

    #[test]
    fn test_parse_error_failed() {
        let err = ParseError::Failed { position: 42 };
        assert!(err.to_string().contains("42"));
        assert!(err.to_string().contains("failed"));
    }

    #[test]
    fn test_parse_error_incomplete() {
        let err = ParseError::Incomplete {
            expected: 100,
            actual: 50,
        };
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("50"));
        assert!(err.to_string().contains("incomplete"));
    }

    #[test]
    fn test_parse_error_invalid_grammar() {
        let err = ParseError::InvalidGrammar {
            reason: "missing root".to_string(),
        };
        assert!(err.to_string().contains("missing root"));
        assert!(err.to_string().contains("Invalid grammar"));
    }

    #[test]
    fn test_parse_error_internal() {
        let err = ParseError::Internal {
            message: "unexpected state".to_string(),
        };
        assert!(err.to_string().contains("unexpected state"));
        assert!(err.to_string().contains("Internal error"));
    }

    #[test]
    fn test_parse_error_input_too_large() {
        let err = ParseError::InputTooLarge {
            input_size: 1000000,
            max_size: 500000,
        };
        assert!(err.to_string().contains("1000000"));
        assert!(err.to_string().contains("500000"));
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_parse_error_recursion_limit_exceeded() {
        let err = ParseError::RecursionLimitExceeded {
            depth: 1500,
            max_depth: 1000,
        };
        assert!(err.to_string().contains("1500"));
        assert!(err.to_string().contains("1000"));
        assert!(err.to_string().contains("Recursion limit"));
    }

    #[test]
    fn test_parse_error_timeout_exceeded() {
        let err = ParseError::TimeoutExceeded {
            elapsed_ms: 5000,
            timeout_ms: 1000,
        };
        assert!(err.to_string().contains("5000"));
        assert!(err.to_string().contains("1000"));
        assert!(err.to_string().contains("Timeout"));
    }

    #[test]
    fn test_parse_error_memory_limit_exceeded() {
        let err = ParseError::MemoryLimitExceeded {
            used_bytes: 50000000,
            max_bytes: 10000000,
        };
        assert!(err.to_string().contains("50000000"));
        assert!(err.to_string().contains("10000000"));
        assert!(err.to_string().contains("Memory limit"));
    }

    #[test]
    fn test_parse_error_at_position() {
        let err = ParseError::at_position(42);
        match err {
            ParseError::Failed { position } => assert_eq!(position, 42),
            _ => panic!("Expected Failed variant"),
        }
    }

    #[test]
    fn test_parse_error_format_with_position() {
        let input = "hello\nworld";
        let err = ParseError::Failed { position: 7 };
        let formatted = err.format_with_position(input);
        assert!(formatted.contains("line 2"));
        assert!(formatted.contains("column 2"));
    }

    #[test]
    fn test_parse_error_format_with_position_incomplete() {
        let input = "hello";
        let err = ParseError::Incomplete {
            expected: 10,
            actual: 5,
        };
        let formatted = err.format_with_position(input);
        assert!(formatted.contains("expected 10 bytes"));
        assert!(formatted.contains("parsed 5"));
    }

    // === SourcePosition Tests ===

    #[test]
    fn test_source_position() {
        let pos = SourcePosition {
            offset: 10,
            line: 2,
            column: 5,
        };
        assert_eq!(pos.offset, 10);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 5);
    }

    #[test]
    fn test_source_position_equality() {
        let pos1 = SourcePosition {
            offset: 10,
            line: 2,
            column: 5,
        };
        let pos2 = SourcePosition {
            offset: 10,
            line: 2,
            column: 5,
        };
        let pos3 = SourcePosition {
            offset: 11,
            line: 2,
            column: 5,
        };
        assert_eq!(pos1, pos2);
        assert_ne!(pos1, pos3);
    }

    // === offset_to_position Tests ===

    #[test]
    fn test_offset_to_position_start() {
        let input = "hello world";
        let pos = offset_to_position(input, 0);
        assert_eq!(pos.offset, 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_offset_to_position_middle() {
        let input = "hello world";
        let pos = offset_to_position(input, 6);
        assert_eq!(pos.offset, 6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 7);
    }

    #[test]
    fn test_offset_to_position_after_newline() {
        let input = "hello\nworld";
        let pos = offset_to_position(input, 6);
        assert_eq!(pos.offset, 6);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_offset_to_position_multiline() {
        let input = "line1\nline2\nline3";
        let pos = offset_to_position(input, 12);
        assert_eq!(pos.offset, 12);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_offset_to_position_multibyte() {
        let input = "hello 世界";
        let pos = offset_to_position(input, 6); // After space
        assert_eq!(pos.offset, 6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 7);
    }

    #[test]
    fn test_offset_to_position_beyond_end() {
        let input = "hello";
        let pos = offset_to_position(input, 100);
        // SourcePosition::from_offset clamps offset to input length
        assert_eq!(pos.offset, 5); // Clamped to input length
                                   // Line/column calculation should handle gracefully
        assert!(pos.line >= 1);
        assert!(pos.column >= 1);
    }

    #[test]
    fn test_offset_to_position_empty_input() {
        let input = "";
        let pos = offset_to_position(input, 0);
        assert_eq!(pos.offset, 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    // === Edge Cases ===

    #[test]
    fn test_ast_node_copy() {
        let node = AstNode::Int(42);
        let node_copy = node; // Should compile (Copy trait)
        assert_eq!(node, node_copy);
    }

    #[test]
    #[allow(clippy::clone_on_copy)]
    fn test_ast_node_clone() {
        let node = AstNode::Int(42);
        let node_clone = node.clone();
        assert_eq!(node, node_clone);
    }

    #[test]
    fn test_parse_error_is_std_error() {
        let err = ParseError::Failed { position: 0 };
        let _: &dyn std::error::Error = &err; // Should compile
    }
}
