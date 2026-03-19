//! Clean AST Normalization for Parsanol
//!
//! This module provides optional normalization of the raw native AST to a cleaner,
//! more idiomatic Ruby format. It is NOT Expressir or Parslet specific - it simply
//! provides a more convenient format for Ruby consumers.
//!
//! # Normalization Rules (Universal, Not Domain-Specific)
//!
//! 1. **Character joining**: `["a"@0, "b"@1, "c"@2]` → `"abc"@0`
//!    - Joins consecutive character Slices into a single Slice
//!    - Preserves position from first character
//!
//! 2. **Empty spaces removal**: `{"spaces" => []}` → removed
//!    - Removes empty spaces arrays that don't contribute meaning
//!
//! 3. **Symbol keys**: `{"name" => value}` → `{:name => value}`
//!    - Converts string keys to symbols (Ruby idiom)
//!
//! # No Domain-Specific Transformations
//!
//! This module does NOT perform:
//! - Sequence merging (domain-specific)
//! - Repetition wrapping (domain-specific)
//! - Wrapper pattern detection (domain-specific)
//!
//! Those transformations should be done by the consumer (e.g., Expressir).

use magnus::{value::ReprValue, Class, Error, IntoValue, Module, RArray, Ruby, Value};

use crate::portable::{AstArena, AstNode};

/// Get the Parsanol::Slice class
pub fn get_slice_class(ruby: &Ruby) -> Result<magnus::RClass, Error> {
    let parsanol_module: magnus::RModule =
        ruby.class_object().const_get("Parsanol").map_err(|e| {
            Error::new(
                ruby.exception_runtime_error(),
                format!("Parsanol module not found: {}", e),
            )
        })?;

    parsanol_module.const_get("Slice").map_err(|e| {
        Error::new(
            ruby.exception_runtime_error(),
            format!("Parsanol::Slice class not found: {}", e),
        )
    })
}

/// Create a Parsanol::Slice object with lazy line/column support
///
/// The Slice stores the input string reference so it can compute
/// line/column lazily on demand.
pub fn create_slice(ruby: &Ruby, offset: u32, content: &str, input: &str) -> Result<Value, Error> {
    let slice_class = get_slice_class(ruby)?;
    let offset_val = ruby.integer_from_i64(offset as i64);
    let content_val = ruby.str_new(content);
    let input_val = ruby.str_new(input);

    // Slice.new(offset, content, input) - input stored for lazy line/col computation
    slice_class.new_instance((offset_val, content_val, input_val))
}

/// Check if a value is a Parsanol::Slice
fn is_slice(value: Value, ruby: &Ruby) -> bool {
    if let Ok(slice_class) = get_slice_class(ruby) {
        return value.is_kind_of(slice_class);
    }
    false
}

/// Get content from a Slice or string
fn slice_content(value: Value) -> String {
    if let Ok(s) = value.funcall::<_, _, String>("to_s", ()) {
        return s;
    }
    String::new()
}

/// Get length of a Slice or string
fn slice_length(value: Value) -> usize {
    if let Ok(len) = value.funcall::<_, _, i64>("length", ()) {
        return len as usize;
    }
    0
}

/// Get offset from a Slice
fn slice_offset(value: Value) -> Result<u32, Error> {
    let offset: i64 = value.funcall("offset", ())?;
    Ok(offset as u32)
}

/// Check if value is a Slice or string
fn is_slice_or_string(value: Value, ruby: &Ruby) -> bool {
    is_slice(value, ruby) || value.is_kind_of(ruby.class_string())
}

/// Check if all items in an array are single-character Slices/strings
fn all_single_char_slices(items: &[Value], ruby: &Ruby) -> bool {
    items
        .iter()
        .all(|item| is_slice_or_string(*item, ruby) && slice_length(*item) == 1)
}

/// Join consecutive character Slices into a single Slice
fn join_slices(items: &[Value], ruby: &Ruby, input: &str) -> Result<Value, Error> {
    let first_slice = items.iter().find(|i| is_slice(**i, ruby));
    let content: String = items.iter().map(|i| slice_content(*i)).collect();

    if let Some(first) = first_slice {
        let offset = slice_offset(*first)?;
        create_slice(ruby, offset, &content, input)
    } else {
        Ok(ruby.str_new(&content).as_value())
    }
}

/// Normalize AstNode to clean Ruby format with lazy line/column support
///
/// This performs universal transformations:
/// - Joins character arrays into strings
/// - Converts string keys to symbols
/// - Removes empty spaces arrays
///
/// Slice objects store the input string for lazy line/column computation.
/// Line/column is only calculated when Slice#line_and_column is called.
///
/// It does NOT perform domain-specific transformations like
/// sequence merging or repetition handling.
pub fn normalize_ast(
    node: &AstNode,
    arena: &AstArena,
    input: &str,
    ruby: &Ruby,
) -> Result<Value, Error> {
    match node {
        AstNode::Nil => Ok(ruby.qnil().as_value()),

        AstNode::Bool(b) => Ok((*b).into_value_with(ruby)),

        AstNode::Int(n) => Ok(ruby.integer_from_i64(*n).as_value()),

        AstNode::Float(f) => Ok(ruby.float_from_f64(*f).as_value()),

        AstNode::StringRef { pool_index } => {
            let (s, _, _) = arena.get_string_parts(*pool_index as usize);
            // Interned strings don't have source position, create Slice with offset 0
            create_slice(ruby, 0, s, input)
        }

        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = start + (*length as usize);
            let slice_str = if end <= input.len() {
                &input[start..end]
            } else {
                ""
            };
            create_slice(ruby, *offset, slice_str, input)
        }

        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);

            // Check for :sequence or :repetition tags (pass through, don't transform)
            if let Some(AstNode::StringRef {
                pool_index: tag_idx,
            }) = items.first()
            {
                let (tag, _, _) = arena.get_string_parts(*tag_idx as usize);
                if tag == ":sequence" || tag == ":repetition" {
                    // Return as tagged array (consumer decides how to handle)
                    let ary = ruby.ary_new_capa((items.len()) as _);
                    for item in &items {
                        let ruby_item = normalize_ast(item, arena, input, ruby)?;
                        ary.push(ruby_item)?;
                    }
                    return Ok(ary.as_value());
                }
            }

            // Transform each item
            let transformed: Vec<Value> = items
                .iter()
                .map(|item| normalize_ast(item, arena, input, ruby))
                .collect::<Result<Vec<_>, _>>()?;

            // Check if all items are single-character Slices - join them
            if all_single_char_slices(&transformed, ruby) {
                return join_slices(&transformed, ruby, input);
            }

            // Return as regular array
            let ary = ruby.ary_new_capa(transformed.len() as _);
            for item in &transformed {
                ary.push(*item)?;
            }
            Ok(ary.as_value())
        }

        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let hash = ruby.hash_new();

            for (key, value) in pairs {
                // Skip empty "spaces" keys (universal cleanup)
                if key == "spaces" {
                    // Check if value is empty array
                    if let AstNode::Array { length: 0, .. } = value {
                        continue; // Skip empty spaces
                    }
                }

                // Convert string key to symbol (Ruby idiom)
                let sym_key = ruby.to_symbol(&key);
                let ruby_value = normalize_ast(&value, arena, input, ruby)?;

                // Skip if value is empty array (universal cleanup)
                if let Some(ary) = RArray::from_value(ruby_value) {
                    if ary.is_empty() {
                        continue;
                    }
                }

                hash.aset(sym_key, ruby_value)?;
            }

            Ok(hash.as_value())
        }
    }
}
