//! Zero-copy direct transformation from AST to typed values
//!
//! This module provides the `DirectTransform` trait for transforming `AstNode`
//! directly to typed values without intermediate `Value` allocation.
//! This is a zero-copy alternative to the standard `Transform` system.

use super::super::arena::AstArena;
use super::super::ast::AstNode;
use super::TransformError;

/// Trait for directly transforming AstNode to a typed value without intermediate allocation.
///
/// This provides a zero-copy alternative to the standard Transform system.
/// Instead of: `AstNode -> Value -> YourType`, this goes directly: `AstNode -> YourType`.
///
/// # Example
///
/// ```rust
/// use parsanol::portable::{AstNode, AstArena, DirectTransform, TransformError};
///
/// // Define your type
/// #[derive(Debug, PartialEq)]
/// struct Number(i64);
///
/// // Implement DirectTransform
/// impl DirectTransform for Number {
///     fn from_ast(node: &AstNode, _arena: &AstArena, input: &str) -> Result<Self, TransformError> {
///         match node {
///             AstNode::Int(n) => Ok(Number(*n)),
///             AstNode::InputRef { offset, length } => {
///                 let s = &input[*offset as usize..(*offset + *length) as usize];
///                 s.parse().map(Number).map_err(|_| TransformError::Custom("not a number".into()))
///             }
///             _ => Err(TransformError::TypeMismatch { expected: "number".to_string(), actual: "other".to_string() })
///         }
///     }
/// }
/// ```
pub trait DirectTransform: Sized {
    /// Transform an AstNode directly to this type.
    ///
    /// # Arguments
    /// * `node` - The AST node to transform
    /// * `arena` - The arena containing string/array/hash data
    /// * `input` - The original input string (for InputRef nodes)
    ///
    /// # Returns
    /// The transformed value or an error
    fn from_ast(node: &AstNode, arena: &AstArena, input: &str) -> Result<Self, TransformError>;
}

/// Helper functions for implementing DirectTransform
pub mod direct_helpers {
    use super::{AstArena, AstNode, DirectTransform, TransformError};

    /// Extract a string from an AstNode
    #[inline]
    pub fn extract_string<'a>(
        node: &AstNode,
        arena: &'a AstArena,
        input: &'a str,
    ) -> Result<&'a str, TransformError> {
        match node {
            AstNode::StringRef { pool_index } => Ok(arena.get_string(*pool_index as usize)),
            AstNode::InputRef { offset, length } => {
                let start = *offset as usize;
                let end = start + (*length as usize);
                if end <= input.len() {
                    Ok(&input[start..end])
                } else {
                    Err(TransformError::Custom("InputRef out of bounds".into()))
                }
            }
            _ => Err(TransformError::TypeMismatch {
                expected: "string".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract an integer from an AstNode
    #[inline]
    pub fn extract_int(node: &AstNode) -> Result<i64, TransformError> {
        match node {
            AstNode::Int(n) => Ok(*n),
            _ => Err(TransformError::TypeMismatch {
                expected: "int".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract a float from an AstNode
    #[inline]
    pub fn extract_float(node: &AstNode) -> Result<f64, TransformError> {
        match node {
            AstNode::Float(f) => Ok(*f),
            AstNode::Int(n) => Ok(*n as f64),
            _ => Err(TransformError::TypeMismatch {
                expected: "float".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract a boolean from an AstNode
    #[inline]
    pub fn extract_bool(node: &AstNode) -> Result<bool, TransformError> {
        match node {
            AstNode::Bool(b) => Ok(*b),
            _ => Err(TransformError::TypeMismatch {
                expected: "bool".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract an array from an AstNode
    #[inline]
    pub fn extract_array(node: &AstNode, arena: &AstArena) -> Result<Vec<AstNode>, TransformError> {
        match node {
            AstNode::Array { pool_index, length } => {
                Ok(arena.get_array(*pool_index as usize, *length as usize))
            }
            _ => Err(TransformError::TypeMismatch {
                expected: "array".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract a hash field from an AstNode
    #[inline]
    pub fn extract_hash_field(
        node: &AstNode,
        arena: &AstArena,
        _input: &str,
        field: &str,
    ) -> Result<AstNode, TransformError> {
        match node {
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                for (key, value) in pairs {
                    if key == field {
                        return Ok(value);
                    }
                }
                Err(TransformError::MissingField(field.to_string()))
            }
            _ => Err(TransformError::TypeMismatch {
                expected: "hash".into(),
                actual: "other".into(),
            }),
        }
    }

    /// Extract and transform a hash field using DirectTransform
    #[inline]
    pub fn extract_field_as<T: DirectTransform>(
        node: &AstNode,
        arena: &AstArena,
        input: &str,
        field: &str,
    ) -> Result<T, TransformError> {
        let field_node = extract_hash_field(node, arena, input, field)?;
        T::from_ast(&field_node, arena, input)
    }

    /// Transform an array of AstNodes to a `Vec<T>`
    #[inline]
    pub fn transform_array<T: DirectTransform>(
        node: &AstNode,
        arena: &AstArena,
        input: &str,
    ) -> Result<Vec<T>, TransformError> {
        let items = extract_array(node, arena)?;
        items
            .iter()
            .map(|item| T::from_ast(item, arena, input))
            .collect()
    }
}

// Implement DirectTransform for common types

impl DirectTransform for i64 {
    fn from_ast(node: &AstNode, _arena: &AstArena, _input: &str) -> Result<Self, TransformError> {
        direct_helpers::extract_int(node)
    }
}

impl DirectTransform for f64 {
    fn from_ast(node: &AstNode, _arena: &AstArena, _input: &str) -> Result<Self, TransformError> {
        direct_helpers::extract_float(node)
    }
}

impl DirectTransform for bool {
    fn from_ast(node: &AstNode, _arena: &AstArena, _input: &str) -> Result<Self, TransformError> {
        direct_helpers::extract_bool(node)
    }
}

impl DirectTransform for String {
    fn from_ast(node: &AstNode, arena: &AstArena, input: &str) -> Result<Self, TransformError> {
        direct_helpers::extract_string(node, arena, input).map(|s| s.to_string())
    }
}

impl<T: DirectTransform> DirectTransform for Option<T> {
    fn from_ast(node: &AstNode, arena: &AstArena, input: &str) -> Result<Self, TransformError> {
        match node {
            AstNode::Nil => Ok(None),
            _ => T::from_ast(node, arena, input).map(Some),
        }
    }
}

impl<T: DirectTransform> DirectTransform for Vec<T> {
    fn from_ast(node: &AstNode, arena: &AstArena, input: &str) -> Result<Self, TransformError> {
        direct_helpers::transform_array(node, arena, input)
    }
}
