//! Helper functions for transformations
//!
//! This module provides helper functions for converting AST nodes to Values
//! and extracting values from the transformation system.

use std::collections::HashMap;

use super::super::arena::AstArena;
use super::super::ast::AstNode;
use super::{TransformError, Value};

// ============================================================================
// AST to Value conversion
// ============================================================================

/// Convert an AstNode to a Value
pub fn ast_to_value(node: &AstNode, arena: &AstArena, input: &str) -> Value {
    match node {
        AstNode::Nil => Value::Nil,
        AstNode::Bool(b) => Value::Bool(*b),
        AstNode::Int(n) => Value::Int(*n),
        AstNode::Float(f) => Value::Float(f.to_bits() as f64), // Approximate
        AstNode::StringRef { pool_index } => {
            let s = arena.get_string(*pool_index as usize);
            Value::String(s.to_string())
        }
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = start + *length as usize;
            let s = &input[start..end.min(input.len())];
            Value::String(s.to_string())
        }
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            let values: Vec<Value> = items
                .iter()
                .map(|i| ast_to_value(i, arena, input))
                .collect();
            Value::Array(values)
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let mut map = HashMap::new();
            for (k, v) in pairs {
                map.insert(k.clone(), ast_to_value(&v, arena, input));
            }
            Value::Hash(map)
        }
        AstNode::Tagged { tag: _, value } => {
            // Tagged nodes should be handled by to_parslet_compatible transformation
            // For now, just return the inner value
            ast_to_value(value, arena, input)
        }
    }
}

/// Get the source span for an AST node, if available
///
/// Returns None for leaf nodes without position info (Nil, Bool, Int, Float)
pub fn ast_node_span(
    node: &AstNode,
    input: &str,
) -> Option<super::super::source_location::SourceSpan> {
    match node {
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            Some(super::super::source_location::SourceSpan::from_offsets(
                input,
                start,
                start + *length as usize,
            ))
        }
        AstNode::StringRef { pool_index: _ } => {
            // StringRef references arena interned strings, which don't have positions
            // Could be enhanced to track position at intern time
            None
        }
        AstNode::Array {
            pool_index: _,
            length: _,
        } => {
            // Could compute span from children - for now, return None
            None
        }
        AstNode::Hash {
            pool_index: _,
            length: _,
        } => {
            // Could compute span from children - for now, return None
            None
        }
        AstNode::Tagged { tag: _, value } => {
            // Delegate to inner value's span
            ast_node_span(value, input)
        }
        AstNode::Nil | AstNode::Bool(_) | AstNode::Int(_) | AstNode::Float(_) => None,
    }
}

/// Convert an AstNode to a Value with source span information
///
/// This is useful for tracking original source positions through transformations,
/// enabling features like IDE integration and precise error reporting.
pub fn ast_to_value_with_span(
    node: &AstNode,
    arena: &AstArena,
    input: &str,
) -> super::super::source_map::SourceMapped<Value> {
    let value = ast_to_value(node, arena, input);
    let span = ast_node_span(node, input)
        .unwrap_or_else(|| super::super::source_location::SourceSpan::start());
    super::super::source_map::SourceMapped::new(value, span)
}

// ============================================================================
// Value extraction helpers
// ============================================================================

/// Extract an integer from a value
pub fn extract_int(value: &Value) -> Result<i64, TransformError> {
    value.as_int().ok_or_else(|| TransformError::TypeMismatch {
        expected: "int".to_string(),
        actual: format!("{:?}", value),
    })
}

/// Extract a string from a value
pub fn extract_string(value: &Value) -> Result<String, TransformError> {
    value
        .to_string()
        .ok_or_else(|| TransformError::TypeMismatch {
            expected: "string".to_string(),
            actual: format!("{:?}", value),
        })
}

/// Extract a hash field
pub fn extract_field<'a>(value: &'a Value, field: &str) -> Result<&'a Value, TransformError> {
    value
        .get(field)
        .ok_or_else(|| TransformError::MissingField(field.to_string()))
}
