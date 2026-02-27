//! Streaming builder API for single-pass parsing
//!
//! This module provides a trait-based interface for consuming parse results
//! during parsing, eliminating the need for intermediate AST construction.
//!
//! # Overview
//!
//! The StreamingBuilder trait allows consumers to receive parse events
//! as they happen, enabling:
//! - Single-pass parsing (no intermediate AST)
//! - Custom output construction
//! - Memory-efficient streaming
//!
//! # Example
//!
//! ```
//! use parsanol::portable::streaming_builder::{StreamingBuilder, BuildResult, BuildError};
//!
//! // Custom builder that collects strings
//! struct StringCollector {
//!     strings: Vec<String>,
//! }
//!
//! impl StreamingBuilder for StringCollector {
//!     type Output = Vec<String>;
//!
//!     fn on_string(&mut self, value: &str, _offset: usize, _length: usize) -> BuildResult<()> {
//!         self.strings.push(value.to_string());
//!         Ok(())
//!     }
//!
//!     fn finish(&mut self) -> BuildResult<Vec<String>> {
//!         Ok(std::mem::take(&mut self.strings))
//!     }
//! }
//!
//! let mut builder = StringCollector { strings: vec![] };
//! builder.on_string("hello", 0, 5).unwrap();
//! builder.on_string("world", 6, 5).unwrap();
//! let result = builder.finish().unwrap();
//! assert_eq!(result, vec!["hello", "world"]);
//! ```

use super::ast::ParseError;

/// Result of a builder operation
pub type BuildResult<T> = Result<T, BuildError>;

/// Errors that can occur during building
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// Invalid structure encountered
    InvalidStructure {
        /// Description of the invalid structure
        message: String,
    },
    /// Type mismatch
    TypeMismatch {
        /// Expected type
        expected: String,
        /// Actual type found
        actual: String,
    },
    /// Missing required field
    MissingField {
        /// Name of the missing field
        field: String,
    },
    /// Custom error from builder
    Custom {
        /// Custom error message
        message: String,
    },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::InvalidStructure { message } => {
                write!(f, "Invalid structure: {}", message)
            }
            BuildError::TypeMismatch { expected, actual } => {
                write!(f, "Type mismatch: expected {}, got {}", expected, actual)
            }
            BuildError::MissingField { field } => {
                write!(f, "Missing required field: {}", field)
            }
            BuildError::Custom { message } => {
                write!(f, "Build error: {}", message)
            }
        }
    }
}

impl std::error::Error for BuildError {}

impl From<BuildError> for ParseError {
    fn from(e: BuildError) -> Self {
        ParseError::BuilderError {
            message: e.to_string(),
        }
    }
}

/// Trait for streaming parse result construction
///
/// Implement this trait to receive parse events during parsing.
/// Each method returns `BuildResult<()>` to allow error handling.
///
/// # Event Flow
///
/// ```text
/// on_named_start("entity")
///   on_string("EntityName", 0, 10)
///   on_array_start(3)
///     on_hash_start(2)
///       on_string("name", 0, 4)
///       on_named_start("attribute")
///         on_string("id", 5, 2)
///       on_named_end("attribute")
///       on_string("type", 8, 6)
///     on_hash_end(2)
///   on_array_end(3)
/// on_named_end("entity")
/// ```
///
/// # Error Handling
///
/// Return `Err(BuildError::...)` to abort parsing with an error.
/// The parser will stop immediately and return the error.
///
/// # Generic Output
///
/// Each builder defines its own `Output` type. This allows:
/// - Building domain-specific types directly
/// - Returning collections, single values, or complex structures
/// - Zero-copy construction when possible
///
pub trait StreamingBuilder {
    /// The output type produced by this builder
    type Output;

    // === Named Captures ===

    /// Called when a named capture starts
    ///
    /// This is called when a named sequence or repetition starts.
    /// Use this to push onto a stack or initialize state.
    ///
    /// # Arguments
    /// * `name` - The capture name (from `.as(:name)`)
    ///
    /// # Example
    /// ```ignore
    /// fn on_named_start(&mut self, name: &str) -> BuildResult<()> {
    ///     self.stack.push(self.current_object.clone());
    ///     Ok(())
    /// }
    /// ```
    fn on_named_start(&mut self, name: &str) -> BuildResult<()> {
        let _ = name;
        Ok(())
    }

    /// Called when a named capture ends
    ///
    /// This is called when a named sequence or repetition ends.
    /// Use this to finalize the current object and pop from stack.
    ///
    /// # Arguments
    /// * `name` - The capture name (from `.as(:name)`)
    ///
    fn on_named_end(&mut self, name: &str) -> BuildResult<()> {
        let _ = name;
        Ok(())
    }

    // === Primitive Values ===

    /// Called when a string value is parsed
    ///
    /// # Arguments
    /// * `value` - The string content
    /// * `offset` - Byte offset in input (for position tracking)
    /// * `length` - Byte length (for position tracking)
    ///
    /// # Note
    /// The builder receives the actual string value, NOT a Slice object.
    /// Use offset/length for position information if needed.
    ///
    fn on_string(&mut self, value: &str, offset: usize, length: usize) -> BuildResult<()> {
        let _ = (value, offset, length);
        Ok(())
    }

    /// Called when an integer value is parsed
    fn on_int(&mut self, value: i64) -> BuildResult<()> {
        let _ = value;
        Ok(())
    }

    /// Called when a float value is parsed
    fn on_float(&mut self, value: f64) -> BuildResult<()> {
        let _ = value;
        Ok(())
    }

    /// Called when a boolean value is parsed
    fn on_bool(&mut self, value: bool) -> BuildResult<()> {
        let _ = value;
        Ok(())
    }

    /// Called when a nil value is parsed
    fn on_nil(&mut self) -> BuildResult<()> {
        Ok(())
    }

    // === Compound Values ===

    /// Called when an array starts
    ///
    /// # Arguments
    /// * `expected_len` - Expected number of elements (if known)
    ///
    fn on_array_start(&mut self, expected_len: Option<usize>) -> BuildResult<()> {
        let _ = expected_len;
        Ok(())
    }

    /// Called for each array element
    ///
    /// # Note
    /// This is called AFTER the element's events have been processed.
    /// Use this to collect elements into the current array.
    ///
    fn on_array_element(&mut self, index: usize) -> BuildResult<()> {
        let _ = index;
        Ok(())
    }

    /// Called when an array ends
    ///
    /// # Arguments
    /// * `actual_len` - Actual number of elements received
    ///
    fn on_array_end(&mut self, actual_len: usize) -> BuildResult<()> {
        let _ = actual_len;
        Ok(())
    }

    /// Called when a hash starts
    fn on_hash_start(&mut self, expected_len: Option<usize>) -> BuildResult<()> {
        let _ = expected_len;
        Ok(())
    }

    /// Called for each hash key
    fn on_hash_key(&mut self, key: &str) -> BuildResult<()> {
        let _ = key;
        Ok(())
    }

    /// Called for each hash value (after the value's events)
    fn on_hash_value(&mut self, key: &str) -> BuildResult<()> {
        let _ = key;
        Ok(())
    }

    /// Called when a hash ends
    fn on_hash_end(&mut self, actual_len: usize) -> BuildResult<()> {
        let _ = actual_len;
        Ok(())
    }

    // === Lifecycle ===

    /// Called before parsing starts
    fn on_start(&mut self, input: &str) -> BuildResult<()> {
        let _ = input;
        Ok(())
    }

    /// Called after parsing succeeds
    fn on_success(&mut self) -> BuildResult<()> {
        Ok(())
    }

    /// Called after parsing fails
    fn on_error(&mut self, error: &ParseError) -> BuildResult<()> {
        let _ = error;
        Ok(())
    }

    /// Finalize and return the built result
    ///
    /// This is called after `on_success` to get the final output.
    ///
    fn finish(&mut self) -> BuildResult<Self::Output>
    where
        Self::Output: Sized;
}

// ============================================================================
// Built-in Builders
// ============================================================================

/// A builder that collects events for debugging
///
/// # Example
///
/// ```
/// use parsanol::portable::streaming_builder::{DebugBuilder, StreamingBuilder};
///
/// let mut builder = DebugBuilder::new();
/// builder.on_string("hello", 0, 5).unwrap();
/// builder.on_int(42).unwrap();
/// let events = builder.finish().unwrap();
/// assert_eq!(events, vec!["string(\"hello\" @ 0 len=5)", "int(42)"]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct DebugBuilder {
    /// List of collected event strings
    pub events: Vec<String>,
}

impl DebugBuilder {
    /// Create a new debug builder
    pub fn new() -> Self {
        Self::default()
    }
}

impl StreamingBuilder for DebugBuilder {
    type Output = Vec<String>;

    fn on_named_start(&mut self, name: &str) -> BuildResult<()> {
        self.events.push(format!("named_start({})", name));
        Ok(())
    }

    fn on_named_end(&mut self, name: &str) -> BuildResult<()> {
        self.events.push(format!("named_end({})", name));
        Ok(())
    }

    fn on_string(&mut self, value: &str, offset: usize, length: usize) -> BuildResult<()> {
        self.events
            .push(format!("string({:?} @ {} len={})", value, offset, length));
        Ok(())
    }

    fn on_int(&mut self, value: i64) -> BuildResult<()> {
        self.events.push(format!("int({})", value));
        Ok(())
    }

    fn on_float(&mut self, value: f64) -> BuildResult<()> {
        self.events.push(format!("float({})", value));
        Ok(())
    }

    fn on_bool(&mut self, value: bool) -> BuildResult<()> {
        self.events.push(format!("bool({})", value));
        Ok(())
    }

    fn on_nil(&mut self) -> BuildResult<()> {
        self.events.push("nil".to_string());
        Ok(())
    }

    fn on_array_start(&mut self, expected_len: Option<usize>) -> BuildResult<()> {
        self.events.push(format!("array_start({:?})", expected_len));
        Ok(())
    }

    fn on_array_element(&mut self, index: usize) -> BuildResult<()> {
        self.events.push(format!("array_element({})", index));
        Ok(())
    }

    fn on_array_end(&mut self, actual_len: usize) -> BuildResult<()> {
        self.events.push(format!("array_end({})", actual_len));
        Ok(())
    }

    fn on_hash_start(&mut self, expected_len: Option<usize>) -> BuildResult<()> {
        self.events.push(format!("hash_start({:?})", expected_len));
        Ok(())
    }

    fn on_hash_key(&mut self, key: &str) -> BuildResult<()> {
        self.events.push(format!("hash_key({})", key));
        Ok(())
    }

    fn on_hash_value(&mut self, key: &str) -> BuildResult<()> {
        self.events.push(format!("hash_value({})", key));
        Ok(())
    }

    fn on_hash_end(&mut self, actual_len: usize) -> BuildResult<()> {
        self.events.push(format!("hash_end({})", actual_len));
        Ok(())
    }

    fn finish(&mut self) -> BuildResult<Vec<String>> {
        Ok(std::mem::take(&mut self.events))
    }
}

/// A builder that collects all strings from the parse
///
/// # Example
///
/// ```
/// use parsanol::portable::streaming_builder::{BuilderStringCollector, StreamingBuilder};
///
/// let mut builder = BuilderStringCollector::new();
/// builder.on_string("hello", 0, 5).unwrap();
/// builder.on_string("world", 6, 5).unwrap();
/// let strings = builder.finish().unwrap();
/// assert_eq!(strings, vec!["hello", "world"]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct BuilderStringCollector {
    /// Collected strings
    pub strings: Vec<String>,
}

impl BuilderStringCollector {
    /// Create a new string collector
    pub fn new() -> Self {
        Self::default()
    }
}

impl StreamingBuilder for BuilderStringCollector {
    type Output = Vec<String>;

    fn on_string(&mut self, value: &str, _offset: usize, _length: usize) -> BuildResult<()> {
        self.strings.push(value.to_string());
        Ok(())
    }

    fn finish(&mut self) -> BuildResult<Vec<String>> {
        Ok(std::mem::take(&mut self.strings))
    }
}

/// A builder that counts nodes of each type
///
/// # Example
///
/// ```
/// use parsanol::portable::streaming_builder::{BuilderNodeCounter, StreamingBuilder};
///
/// let mut counter = BuilderNodeCounter::new();
/// counter.on_string("hello", 0, 5).unwrap();
/// counter.on_int(42).unwrap();
/// counter.on_int(100).unwrap();
/// counter.on_bool(true).unwrap();
/// counter.finish().unwrap();
///
/// assert_eq!(counter.strings, 1);
/// assert_eq!(counter.ints, 2);
/// assert_eq!(counter.bools, 1);
/// ```
#[derive(Debug, Clone, Default)]
pub struct BuilderNodeCounter {
    /// Number of named start events
    pub named_starts: usize,
    /// Number of named end events
    pub named_ends: usize,
    /// Number of string values
    pub strings: usize,
    /// Number of integer values
    pub ints: usize,
    /// Number of float values
    pub floats: usize,
    /// Number of boolean values
    pub bools: usize,
    /// Number of nil values
    pub nils: usize,
    /// Number of array starts
    pub arrays: usize,
    /// Number of hash starts
    pub hashes: usize,
}

impl BuilderNodeCounter {
    /// Create a new node counter
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the total number of nodes
    pub fn total(&self) -> usize {
        self.strings + self.ints + self.floats + self.bools + self.nils + self.arrays + self.hashes
    }
}

impl StreamingBuilder for BuilderNodeCounter {
    type Output = ();

    fn on_named_start(&mut self, _name: &str) -> BuildResult<()> {
        self.named_starts += 1;
        Ok(())
    }

    fn on_named_end(&mut self, _name: &str) -> BuildResult<()> {
        self.named_ends += 1;
        Ok(())
    }

    fn on_string(&mut self, _value: &str, _offset: usize, _length: usize) -> BuildResult<()> {
        self.strings += 1;
        Ok(())
    }

    fn on_int(&mut self, _value: i64) -> BuildResult<()> {
        self.ints += 1;
        Ok(())
    }

    fn on_float(&mut self, _value: f64) -> BuildResult<()> {
        self.floats += 1;
        Ok(())
    }

    fn on_bool(&mut self, _value: bool) -> BuildResult<()> {
        self.bools += 1;
        Ok(())
    }

    fn on_nil(&mut self) -> BuildResult<()> {
        self.nils += 1;
        Ok(())
    }

    fn on_array_start(&mut self, _expected_len: Option<usize>) -> BuildResult<()> {
        self.arrays += 1;
        Ok(())
    }

    fn on_hash_start(&mut self, _expected_len: Option<usize>) -> BuildResult<()> {
        self.hashes += 1;
        Ok(())
    }

    fn finish(&mut self) -> BuildResult<()> {
        Ok(())
    }
}

/// A builder that tracks maximum nesting depth
///
/// # Example
///
/// ```
/// use parsanol::portable::streaming_builder::{DepthTracker, StreamingBuilder};
///
/// let mut tracker = DepthTracker::new();
/// tracker.on_array_start(None).unwrap();
///   tracker.on_array_start(None).unwrap();
///     tracker.on_array_start(None).unwrap();
///     tracker.on_array_end(0).unwrap();
///   tracker.on_array_end(0).unwrap();
/// tracker.on_array_end(0).unwrap();
/// tracker.finish().unwrap();
///
/// assert_eq!(tracker.max_depth, 3);
/// ```
#[derive(Debug, Clone, Default)]
pub struct DepthTracker {
    /// Current nesting depth
    pub current_depth: usize,
    /// Maximum nesting depth encountered
    pub max_depth: usize,
}

impl DepthTracker {
    /// Create a new depth tracker
    #[allow(clippy::only_used_in_recursion)]
    pub fn new() -> Self {
        Self::default()
    }
}

impl StreamingBuilder for DepthTracker {
    type Output = usize;

    fn on_array_start(&mut self, _expected_len: Option<usize>) -> BuildResult<()> {
        self.current_depth += 1;
        self.max_depth = self.max_depth.max(self.current_depth);
        Ok(())
    }

    fn on_array_end(&mut self, _actual_len: usize) -> BuildResult<()> {
        self.current_depth = self.current_depth.saturating_sub(1);
        Ok(())
    }

    fn on_hash_start(&mut self, _expected_len: Option<usize>) -> BuildResult<()> {
        self.current_depth += 1;
        self.max_depth = self.max_depth.max(self.current_depth);
        Ok(())
    }

    fn on_hash_end(&mut self, _actual_len: usize) -> BuildResult<()> {
        self.current_depth = self.current_depth.saturating_sub(1);
        Ok(())
    }

    fn finish(&mut self) -> BuildResult<usize> {
        Ok(self.max_depth)
    }
}

/// Walk an AST with a streaming builder (post-parse conversion)
///
/// This utility function converts an already-built AST into builder events.
/// Useful for reusing builder implementations with pre-parsed ASTs.
///
/// # Example
///
/// ```
/// use parsanol::portable::streaming_builder::{walk_ast, DebugBuilder, StreamingBuilder};
/// use parsanol::portable::{AstArena, AstNode};
///
/// let mut arena = AstArena::new();
/// // ... parse and get ast_node ...
///
/// let mut builder = DebugBuilder::new();
/// // walk_ast(&ast_node, &arena, "input", &mut builder);
/// // let events = builder.finish().unwrap();
/// ```
pub fn walk_ast<B: StreamingBuilder>(
    node: &super::ast::AstNode,
    arena: &super::arena::AstArena,
    input: &str,
    builder: &mut B,
) -> BuildResult<()> {
    walk_ast_inner(node, arena, input, builder, 0)
}

#[allow(clippy::only_used_in_recursion)]
fn walk_ast_inner<B: StreamingBuilder>(
    node: &super::ast::AstNode,
    arena: &super::arena::AstArena,
    input: &str,
    builder: &mut B,
    depth: usize,
) -> BuildResult<()> {
    match node {
        super::ast::AstNode::Nil => {
            builder.on_nil()?;
        }
        super::ast::AstNode::Bool(value) => {
            builder.on_bool(*value)?;
        }
        super::ast::AstNode::Int(value) => {
            builder.on_int(*value)?;
        }
        super::ast::AstNode::Float(value) => {
            builder.on_float(*value)?;
        }
        super::ast::AstNode::StringRef { pool_index } => {
            let value = arena.get_string(*pool_index as usize);
            builder.on_string(value, 0, value.len())?;
        }
        super::ast::AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = start + (*length as usize);
            if end <= input.len() {
                let value = &input[start..end];
                builder.on_string(value, start, *length as usize)?;
            }
        }
        super::ast::AstNode::Array { pool_index, length } => {
            let expected = if *length > 0 {
                Some(*length as usize)
            } else {
                None
            };
            builder.on_array_start(expected)?;

            let items = arena.get_array(*pool_index as usize, *length as usize);
            for (i, item) in items.iter().enumerate() {
                walk_ast_inner(item, arena, input, builder, depth + 1)?;
                builder.on_array_element(i)?;
            }

            builder.on_array_end(*length as usize)?;
        }
        super::ast::AstNode::Hash { pool_index, length } => {
            let expected = if *length > 0 {
                Some(*length as usize)
            } else {
                None
            };
            builder.on_hash_start(expected)?;

            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            for (key, value) in pairs {
                builder.on_hash_key(&key)?;
                walk_ast_inner(&value, arena, input, builder, depth + 1)?;
                builder.on_hash_value(&key)?;
            }

            builder.on_hash_end(*length as usize)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_builder() {
        let mut builder = DebugBuilder::new();
        builder.on_string("hello", 0, 5).unwrap();
        builder.on_int(42).unwrap();
        builder.on_bool(true).unwrap();
        builder.on_nil().unwrap();

        let events = builder.finish().unwrap();
        assert_eq!(events.len(), 4);
        assert!(events[0].contains("hello"));
        assert!(events[1].contains("42"));
        assert!(events[2].contains("true"));
        assert!(events[3].contains("nil"));
    }

    #[test]
    fn test_string_collector() {
        let mut builder = BuilderStringCollector::new();
        builder.on_string("first", 0, 5).unwrap();
        builder.on_string("second", 6, 6).unwrap();
        builder.on_int(42).unwrap(); // Should be ignored

        let strings = builder.finish().unwrap();
        assert_eq!(strings, vec!["first", "second"]);
    }

    #[test]
    fn test_node_counter() {
        let mut counter = BuilderNodeCounter::new();

        counter.on_string("test", 0, 4).unwrap();
        counter.on_int(1).unwrap();
        counter.on_int(2).unwrap();
        counter.on_float(1.5).unwrap();
        counter.on_bool(true).unwrap();
        counter.on_nil().unwrap();
        counter.on_array_start(Some(3)).unwrap();
        counter.on_array_end(3).unwrap();
        counter.on_hash_start(Some(2)).unwrap();
        counter.on_hash_end(2).unwrap();

        counter.finish().unwrap();

        assert_eq!(counter.strings, 1);
        assert_eq!(counter.ints, 2);
        assert_eq!(counter.floats, 1);
        assert_eq!(counter.bools, 1);
        assert_eq!(counter.nils, 1);
        assert_eq!(counter.arrays, 1);
        assert_eq!(counter.hashes, 1);
        assert_eq!(counter.total(), 8);
    }

    #[test]
    fn test_depth_tracker() {
        let mut tracker = DepthTracker::new();

        // Depth 1
        tracker.on_array_start(None).unwrap();
        assert_eq!(tracker.current_depth, 1);
        assert_eq!(tracker.max_depth, 1);

        // Depth 2
        tracker.on_array_start(None).unwrap();
        assert_eq!(tracker.current_depth, 2);
        assert_eq!(tracker.max_depth, 2);

        // Depth 3
        tracker.on_hash_start(None).unwrap();
        assert_eq!(tracker.current_depth, 3);
        assert_eq!(tracker.max_depth, 3);

        // Back to depth 2
        tracker.on_hash_end(0).unwrap();
        assert_eq!(tracker.current_depth, 2);

        // Back to depth 1
        tracker.on_array_end(0).unwrap();
        assert_eq!(tracker.current_depth, 1);

        // Back to depth 0
        tracker.on_array_end(0).unwrap();
        assert_eq!(tracker.current_depth, 0);

        let max = tracker.finish().unwrap();
        assert_eq!(max, 3);
    }

    #[test]
    fn test_build_error_display() {
        let e1 = BuildError::InvalidStructure {
            message: "test".to_string(),
        };
        assert_eq!(e1.to_string(), "Invalid structure: test");

        let e2 = BuildError::TypeMismatch {
            expected: "int".to_string(),
            actual: "string".to_string(),
        };
        assert_eq!(e2.to_string(), "Type mismatch: expected int, got string");

        let e3 = BuildError::MissingField {
            field: "name".to_string(),
        };
        assert_eq!(e3.to_string(), "Missing required field: name");

        let e4 = BuildError::Custom {
            message: "custom error".to_string(),
        };
        assert_eq!(e4.to_string(), "Build error: custom error");
    }

    #[test]
    fn test_named_capture_tracking() {
        let mut builder = DebugBuilder::new();

        builder.on_named_start("entity").unwrap();
        builder.on_string("EntityName", 0, 10).unwrap();
        builder.on_named_end("entity").unwrap();

        let events = builder.finish().unwrap();
        assert_eq!(
            events,
            vec![
                "named_start(entity)",
                "string(\"EntityName\" @ 0 len=10)",
                "named_end(entity)",
            ]
        );
    }

    #[test]
    fn test_array_tracking() {
        let mut builder = DebugBuilder::new();

        builder.on_array_start(Some(3)).unwrap();
        builder.on_int(1).unwrap();
        builder.on_array_element(0).unwrap();
        builder.on_int(2).unwrap();
        builder.on_array_element(1).unwrap();
        builder.on_int(3).unwrap();
        builder.on_array_element(2).unwrap();
        builder.on_array_end(3).unwrap();

        let events = builder.finish().unwrap();
        assert_eq!(events.len(), 8);
        assert_eq!(events[0], "array_start(Some(3))");
        assert_eq!(events[7], "array_end(3)");
    }

    #[test]
    fn test_hash_tracking() {
        let mut builder = DebugBuilder::new();

        builder.on_hash_start(Some(2)).unwrap();
        builder.on_hash_key("name").unwrap();
        builder.on_string("test", 0, 4).unwrap();
        builder.on_hash_value("name").unwrap();
        builder.on_hash_key("value").unwrap();
        builder.on_int(42).unwrap();
        builder.on_hash_value("value").unwrap();
        builder.on_hash_end(2).unwrap();

        let events = builder.finish().unwrap();
        assert_eq!(events.len(), 8);
        assert_eq!(events[0], "hash_start(Some(2))");
        assert_eq!(events[7], "hash_end(2)");
    }
}
