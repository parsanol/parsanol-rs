//! FFI utilities for cross-language bindings
//!
//! This module provides a unified implementation for flattening AST nodes
//! to flat arrays that can be passed across FFI boundaries (Ruby, WASM, etc.).
//!
//! # Format
//!
//! The flat array format uses u64 values:
//!
//! | Tag | Value | Description |
//! |-----|-------|-------------|
//! | 0x00 | - | nil |
//! | 0x01 | 0 or 1 | bool |
//! | 0x02 | value | int |
//! | 0x03 | IEEE bits | float |
//! | 0x04 | offset, length | string_ref |
//! | 0x05 | ...children... 0x06 | array |
//! | 0x07 | ...key-values... 0x08 | hash |
//! | 0x09 | len, data... | hash_key |

use super::arena::AstArena;
use super::ast::AstNode;

// Tag constants for flat array format
/// Tag for nil values
pub const TAG_NIL: u64 = 0x00;
/// Tag for boolean values (data: 0 or 1)
pub const TAG_BOOL: u64 = 0x01;
/// Tag for integer values (data: value)
pub const TAG_INT: u64 = 0x02;
/// Tag for float values (data: IEEE bits)
pub const TAG_FLOAT: u64 = 0x03;
/// Tag for string references from input (next 2 cells: offset, length)
pub const TAG_STRING: u64 = 0x04;
/// Tag marking the start of an array
pub const TAG_ARRAY_START: u64 = 0x05;
/// Tag marking the end of an array
pub const TAG_ARRAY_END: u64 = 0x06;
/// Tag marking the start of a hash
pub const TAG_HASH_START: u64 = 0x07;
/// Tag marking the end of a hash
pub const TAG_HASH_END: u64 = 0x08;
/// Tag for hash key (next 2 cells: offset, length)
pub const TAG_HASH_KEY: u64 = 0x09;
/// Tag for inline strings (next: len, then u64 chunks of string bytes)
pub const TAG_INLINE_STRING: u64 = 0x0A;

/// Flatten an AST node to a u64 array for FFI
///
/// This is the unified implementation used by all FFI bindings.
/// It converts an AST tree into a flat array that can be efficiently
/// passed across language boundaries.
///
/// # Arguments
///
/// * `node` - The AST node to flatten
/// * `arena` - The arena containing the AST data
/// * `_input` - The input string (unused, kept for API compatibility)
/// * `output` - The output vector to append flattened data to
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::portable::ffi::flatten_ast_to_u64;
///
/// let mut output = Vec::new();
/// flatten_ast_to_u64(&ast, &arena, &input, &mut output);
/// ```
#[allow(clippy::only_used_in_recursion)]
#[inline]
pub fn flatten_ast_to_u64(node: &AstNode, arena: &AstArena, _input: &str, output: &mut Vec<u64>) {
    match node {
        AstNode::Nil => {
            output.push(TAG_NIL);
        }
        AstNode::Bool(true) => {
            output.push(TAG_BOOL);
            output.push(1);
        }
        AstNode::Bool(false) => {
            output.push(TAG_BOOL);
            output.push(0);
        }
        AstNode::Int(n) => {
            output.push(TAG_INT);
            output.push(*n as u64);
        }
        AstNode::Float(f) => {
            output.push(TAG_FLOAT);
            output.push(f.to_bits());
        }
        AstNode::StringRef { pool_index } => {
            // StringRef points to interned strings in the arena's string pool
            // We need to write the actual string content inline
            let (s, _, _) = arena.get_string_parts(*pool_index as usize);
            let bytes = s.as_bytes();
            let len = bytes.len() as u64;
            output.push(TAG_INLINE_STRING);
            output.push(len);

            // Write string bytes as u64 chunks (same format as hash keys)
            let chunks = (bytes.len() + 7) / 8;
            for chunk_idx in 0..chunks {
                let mut chunk: u64 = 0;
                for byte_idx in 0..8 {
                    let idx = chunk_idx * 8 + byte_idx;
                    if idx < bytes.len() {
                        chunk |= (bytes[idx] as u64) << (byte_idx * 8);
                    }
                }
                output.push(chunk);
            }
        }
        AstNode::InputRef { offset, length } => {
            output.push(TAG_STRING);
            output.push(*offset as u64);
            output.push(*length as u64);
        }
        AstNode::Array { pool_index, length } => {
            output.push(TAG_ARRAY_START);
            let items = arena.get_array(*pool_index as usize, *length as usize);
            for item in items {
                flatten_ast_to_u64(&item, arena, _input, output);
            }
            output.push(TAG_ARRAY_END);
        }
        AstNode::Hash { pool_index, length } => {
            output.push(TAG_HASH_START);
            let items = arena.get_hash_items(*pool_index as usize, *length as usize);
            for (key, value) in items {
                // Write hash key tag
                output.push(TAG_HASH_KEY);

                // Write key bytes as u64 chunks
                let key_bytes = key.as_bytes();
                let len = key_bytes.len() as u64;
                output.push(len);

                // Calculate number of u64 chunks needed (ceil(len / 8))
                let chunks = (key_bytes.len() + 7) / 8;
                for chunk_idx in 0..chunks {
                    let mut chunk: u64 = 0;
                    for byte_idx in 0..8 {
                        let idx = chunk_idx * 8 + byte_idx;
                        if idx < key_bytes.len() {
                            chunk |= (key_bytes[idx] as u64) << (byte_idx * 8);
                        }
                    }
                    output.push(chunk);
                }

                // Write the value
                flatten_ast_to_u64(&value, arena, _input, output);
            }
            output.push(TAG_HASH_END);
        }
    }
}

/// Flatten an AST to a new Vec<u64>
///
/// Convenience function that creates a new vector and flattens the AST into it.
#[inline]
pub fn flatten_ast(node: &AstNode, arena: &AstArena, input: &str) -> Vec<u64> {
    let mut result = Vec::new();
    flatten_ast_to_u64(node, arena, input, &mut result);
    result
}

/// Parse and flatten in one step
///
/// Parses input with the given grammar and returns a flattened array.
/// This is the primary entry point for batch FFI operations.
pub fn parse_to_flat(grammar_json: &str, input: &str) -> Result<Vec<u64>, super::ast::ParseError> {
    use super::grammar::Grammar;
    use super::parser::PortableParser;

    let grammar: Grammar =
        serde_json::from_str(grammar_json).map_err(|e| super::ast::ParseError::InvalidGrammar {
            reason: e.to_string(),
        })?;

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let ast = parser.parse()?;

    Ok(flatten_ast(&ast, &arena, input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flatten_nil() {
        let arena = AstArena::new();
        let mut output = Vec::new();
        flatten_ast_to_u64(&AstNode::Nil, &arena, "", &mut output);
        assert_eq!(output, vec![TAG_NIL as u64]);
    }

    #[test]
    fn test_flatten_bool() {
        let arena = AstArena::new();

        let mut output = Vec::new();
        flatten_ast_to_u64(&AstNode::Bool(true), &arena, "", &mut output);
        assert_eq!(output, vec![TAG_BOOL as u64, 1]);

        let mut output = Vec::new();
        flatten_ast_to_u64(&AstNode::Bool(false), &arena, "", &mut output);
        assert_eq!(output, vec![TAG_BOOL as u64, 0]);
    }

    #[test]
    fn test_flatten_int() {
        let arena = AstArena::new();
        let mut output = Vec::new();
        flatten_ast_to_u64(&AstNode::Int(42), &arena, "", &mut output);
        assert_eq!(output, vec![TAG_INT as u64, 42]);
    }

    #[test]
    fn test_flatten_float() {
        let arena = AstArena::new();
        let mut output = Vec::new();
        flatten_ast_to_u64(&AstNode::Float(3.14), &arena, "", &mut output);
        assert_eq!(output[0], TAG_FLOAT as u64);
        // Check the bits
        let bits = output[1];
        assert_eq!(f64::from_bits(bits), 3.14);
    }
}
