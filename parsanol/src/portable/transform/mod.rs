//! Transformation System for Parse Trees
//!
//! This module provides tools for transforming generic parse trees into
//! typed Rust data structures, similar to Parslet's transformation system.
//!
//! # Example
//!
//! ```rust
//! use parsanol::portable::transform::*;
//!
//! // Define transform rules
//! let transform = Transform::new()
//!     .rule("int", |v| {
//!         let n = v.as_int().ok_or_else(|| TransformError::Custom("not an int".to_string()))?;
//!         Ok(Value::int(n * 2))
//!     });
//!
//! // Create a value to transform
//! let value = Value::hash(vec![("int", Value::int(21))]);
//!
//! // Apply transform
//! let result = transform.apply(&value).unwrap();
//! assert_eq!(result.as_int(), Some(42));
//! ```
//!
//! # Pattern Matching
//!
//! The transform system supports pattern matching similar to Parslet:
//!
//! ```rust,ignore
//! let transform = Transform::new()
//!     // Match a hash with specific keys
//!     .pattern(
//!         Pattern::hash()
//!             .field("left", Pattern::simple("l"))
//!             .field("op", Pattern::str("+"))
//!             .field("right", Pattern::simple("r")),
//!         |bindings| {
//!             let l = bindings.get_int("l")?;
//!             let r = bindings.get_int("r")?;
//!             Ok(Value::int(l + r))
//!         }
//!     );
//! ```

mod direct;
mod helpers;
mod pattern;
mod transform;
mod value;

// Re-export all public types
pub use direct::{direct_helpers, DirectTransform};
pub use helpers::{
    ast_node_span, ast_to_value, ast_to_value_with_span, extract_field, extract_int, extract_string,
};
pub use pattern::{Bindings, HashPatternBuilder, Pattern};
pub use transform::{Transform, TransformError, TypedTransform};
pub use value::Value;

// ============================================================================
// Pattern Macro
// ============================================================================

/// Declarative pattern matching macro for transforms
///
/// This macro provides a concise syntax for creating patterns similar to Parslet's
/// transformation DSL.
///
/// # Syntax
///
/// - `simple(name)` - Match any leaf value and bind to `name`
/// - `sequence(name)` - Match an array and bind to `name`
/// - `subtree(name)` - Match anything and bind to `name`
/// - `str("value")` - Match a specific string
/// - `int(42)` - Match a specific integer
/// - `bool(true)` - Match a specific boolean
/// - `nil` - Match nil
/// - `{ field1: pattern1, field2: pattern2 }` - Match a hash with fields
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::portable::transform::{Pattern, pattern};
///
/// // Simple pattern
/// let p = pattern!(simple(x));
///
/// // Hash pattern with simple fields
/// let p = pattern!({
///     name: simple(n),
///     value: int(42)
/// });
///
/// // Nested pattern
/// let p = pattern!({
///     left: subtree(l),
///     op: str("+"),
///     right: subtree(r)
/// });
/// ```
#[macro_export]
macro_rules! pattern {
    // Simple pattern
    (simple($var:ident)) => {
        $crate::portable::transform::Pattern::simple(stringify!($var))
    };

    // Sequence pattern
    (sequence($var:ident)) => {
        $crate::portable::transform::Pattern::sequence(stringify!($var))
    };

    // Subtree pattern
    (subtree($var:ident)) => {
        $crate::portable::transform::Pattern::subtree(stringify!($var))
    };

    // String literal pattern
    (str($s:literal)) => {
        $crate::portable::transform::Pattern::str($s)
    };

    // Integer pattern
    (int($n:literal)) => {
        $crate::portable::transform::Pattern::int($n)
    };

    // Boolean pattern
    (bool($b:literal)) => {
        $crate::portable::transform::Pattern::bool($b)
    };

    // Nil pattern
    (nil) => {
        $crate::portable::transform::Pattern::nil()
    };

    // Hash pattern with fields
    ({ $($field:ident : $pattern:tt),* $(,)? }) => {{
        let mut builder = $crate::portable::transform::Pattern::hash();
        $(
            builder = builder.field_pattern(stringify!($field), pattern!($pattern));
        )*
        builder.build()
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::arena::AstArena;
    use crate::portable::ast::AstNode;
    use crate::portable::source_location::SourceSpan;
    use crate::portable::source_map::SourceMapped;

    #[test]
    fn test_value_creation() {
        let v = Value::int(42);
        assert_eq!(v.as_int(), Some(42));

        let v = Value::string("hello");
        assert_eq!(v.as_str(), Some("hello"));

        let v = Value::array(vec![Value::int(1), Value::int(2)]);
        assert_eq!(v.as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn test_value_hash() {
        let v = Value::hash(vec![
            ("name", Value::string("test")),
            ("value", Value::int(42)),
        ]);

        assert_eq!(v.get("name").and_then(|v| v.as_str()), Some("test"));
        assert_eq!(v.get("value").and_then(|v| v.as_int()), Some(42));
    }

    #[test]
    fn test_transform_identity() {
        let transform = Transform::new();
        let value = Value::int(42);
        let result = transform.apply(&value).unwrap();
        assert_eq!(result.as_int(), Some(42));
    }

    #[test]
    fn test_transform_rule() {
        let transform = Transform::new().rule("int", |v| {
            let n = v
                .as_int()
                .ok_or_else(|| TransformError::Custom("not an int".to_string()))?;
            Ok(Value::int(n * 2))
        });

        let value = Value::hash(vec![("int", Value::int(21))]);
        let result = transform.apply(&value).unwrap();
        assert_eq!(result.as_int(), Some(42));
    }

    #[test]
    fn test_extract_helpers() {
        let value = Value::hash(vec![("x", Value::int(10)), ("y", Value::string("test"))]);

        let x = extract_field(&value, "x").unwrap();
        assert_eq!(extract_int(x).unwrap(), 10);

        let y = extract_field(&value, "y").unwrap();
        assert_eq!(extract_string(y).unwrap(), "test");
    }

    // ========================================================================
    // DirectTransform Tests
    // ========================================================================

    #[test]
    fn test_direct_transform_int() {
        let arena = AstArena::new();
        let node = AstNode::Int(42);

        let result: i64 = DirectTransform::from_ast(&node, &arena, "").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_direct_transform_float() {
        let arena = AstArena::new();
        let node = AstNode::Float(1.5);

        let result: f64 = DirectTransform::from_ast(&node, &arena, "").unwrap();
        assert!((result - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_direct_transform_bool() {
        let arena = AstArena::new();
        let node = AstNode::Bool(true);

        let result: bool = DirectTransform::from_ast(&node, &arena, "").unwrap();
        assert!(result);
    }

    #[test]
    fn test_direct_transform_option() {
        let arena = AstArena::new();

        // Test None case
        let nil_node = AstNode::Nil;
        let result: Option<i64> = DirectTransform::from_ast(&nil_node, &arena, "").unwrap();
        assert!(result.is_none());

        // Test Some case
        let int_node = AstNode::Int(42);
        let result: Option<i64> = DirectTransform::from_ast(&int_node, &arena, "").unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_direct_transform_vec() {
        let mut arena = AstArena::new();

        // Create an array node
        let items = vec![AstNode::Int(1), AstNode::Int(2), AstNode::Int(3)];
        let (start, len) = arena.store_array(&items);
        let node = AstNode::Array {
            pool_index: start,
            length: len,
        };

        let result: Vec<i64> = DirectTransform::from_ast(&node, &arena, "").unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_direct_helpers_extract_string() {
        let mut arena = AstArena::new();

        // Test with StringRef
        let node = arena.intern_string("hello");
        let result = direct_helpers::extract_string(&node, &arena, "hello").unwrap();
        assert_eq!(result, "hello");

        // Test with InputRef
        let input = "world";
        let node = arena.input_ref(0, 5);
        let result = direct_helpers::extract_string(&node, &arena, input).unwrap();
        assert_eq!(result, "world");
    }

    #[test]
    fn test_direct_helpers_extract_hash_field() {
        let mut arena = AstArena::new();

        let pairs: Vec<(&str, AstNode)> =
            vec![("name", AstNode::Int(42)), ("value", AstNode::Int(100))];
        let (start, len) = arena.store_hash(&pairs);
        let node = AstNode::Hash {
            pool_index: start,
            length: len,
        };

        let field = direct_helpers::extract_hash_field(&node, &arena, "", "name").unwrap();
        assert_eq!(direct_helpers::extract_int(&field).unwrap(), 42);
    }

    #[test]
    fn test_ast_node_span_input_ref() {
        let input = "hello world";

        // Test InputRef at offset 0
        let node = AstNode::InputRef {
            offset: 0,
            length: 5,
        };
        let span = ast_node_span(&node, input).unwrap();
        assert_eq!(span.start.offset, 0);
        assert_eq!(span.end.offset, 5);
        assert_eq!(span.len(), 5);

        // Test InputRef at offset 6
        let node = AstNode::InputRef {
            offset: 6,
            length: 5,
        };
        let span = ast_node_span(&node, input).unwrap();
        assert_eq!(span.start.offset, 6);
        assert_eq!(span.end.offset, 11);
    }

    #[test]
    fn test_ast_node_span_no_position() {
        // Leaf nodes without position info should return None
        assert!(ast_node_span(&AstNode::Nil, "").is_none());
        assert!(ast_node_span(&AstNode::Bool(true), "").is_none());
        assert!(ast_node_span(&AstNode::Int(42), "").is_none());
        assert!(ast_node_span(&AstNode::Float(1.5), "").is_none());
    }

    #[test]
    fn test_ast_to_value_with_span() {
        let input = "hello world";
        let arena = AstArena::new();

        // Test with InputRef
        let node = AstNode::InputRef {
            offset: 0,
            length: 5,
        };
        let mapped = ast_to_value_with_span(&node, &arena, input);

        // Check the value
        assert_eq!(mapped.inner().as_str(), Some("hello"));

        // Check the span
        assert_eq!(mapped.span().start.offset, 0);
        assert_eq!(mapped.span().end.offset, 5);
    }

    #[test]
    fn test_ast_to_value_with_span_int() {
        let arena = AstArena::new();

        // Test with Int node (no span info)
        let node = AstNode::Int(42);
        let mapped = ast_to_value_with_span(&node, &arena, "");

        // Check the value
        assert_eq!(mapped.inner().as_int(), Some(42));

        // Check the span (should be default start span)
        assert_eq!(mapped.span().start.offset, 0);
        assert!(mapped.span().is_empty());
    }

    #[test]
    fn test_source_mapped_combine() {
        let input = "hello world";
        let span1 = SourceSpan::from_offsets(input, 0, 5);
        let span2 = SourceSpan::from_offsets(input, 6, 11);

        let mapped1 = SourceMapped::new("hello", span1);
        let mapped2 = SourceMapped::new("world", span2);

        // Combine should merge spans
        let combined = mapped1.combine(mapped2, |a, b| format!("{} {}", a, b));

        assert_eq!(*combined, "hello world");
        assert_eq!(combined.span().start.offset, 0);
        assert_eq!(combined.span().end.offset, 11);
    }
}
