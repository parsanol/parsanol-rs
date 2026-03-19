//! Full AST Transformation matching Ruby's CanFlatten module
//!
//! This module provides transformation from raw native AST to clean format.
//! It implements the merge_fold logic to match Ruby's flatten output.

use magnus::{value::ReprValue, Error, IntoValue, RArray, Ruby, Value};

use crate::portable::{AstArena, AstNode};

use super::normalize::{create_slice, get_slice_class};

/// Maximum recursion depth to prevent stack overflow
const MAX_RECURSION_DEPTH: u32 = 500;

/// Check if a value is a valid Ruby object (not nil)
#[inline]
fn is_valid_value(value: Value) -> bool {
    !value.is_nil()
}

/// Check if a value is a Slice
fn is_slice(value: Value, ruby: &Ruby) -> bool {
    if !is_valid_value(value) {
        return false;
    }
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

/// Get offset from a Slice
fn slice_offset(value: Value) -> Result<u32, Error> {
    let offset: i64 = value.funcall("offset", ())?;
    Ok(offset as u32)
}

/// Check if value is a Slice or string
fn is_slice_or_string(value: Value, ruby: &Ruby) -> bool {
    if !is_valid_value(value) {
        return false;
    }
    is_slice(value, ruby) || value.is_kind_of(ruby.class_string())
}

/// Check if all non-nil items in an array are string-like (Slice or String)
fn all_string_like(ary: &RArray, ruby: &Ruby) -> bool {
    let len = ary.len();
    let mut has_non_nil = false;
    for i in 0..len {
        if let Ok(item) = ary.entry::<Value>(i as isize) {
            if !item.is_nil() {
                has_non_nil = true;
                if !is_slice_or_string(item, ruby) {
                    return false;
                }
            }
        }
    }
    has_non_nil
}

/// Join consecutive string-like values into a single Slice (filtering nils)
fn join_slices_from_array(ary: &RArray, ruby: &Ruby, input: &str) -> Result<Value, Error> {
    let len = ary.len();
    let mut content = String::new();
    let mut first_offset: Option<u32> = None;

    for i in 0..len {
        if let Ok(item) = ary.entry::<Value>(i as isize) {
            if item.is_nil() {
                continue;
            }
            if first_offset.is_none() && is_slice(item, ruby) {
                first_offset = Some(slice_offset(item)?);
            }
            content.push_str(&slice_content(item));
        }
    }

    if content.is_empty() {
        return Ok(ruby.qnil().as_value());
    }

    if let Some(offset) = first_offset {
        create_slice(ruby, offset, &content, input)
    } else {
        Ok(ruby.str_new(&content).as_value())
    }
}

/// Merge two values using Ruby's merge_fold logic
fn merge_fold(l: Value, r: Value, ruby: &Ruby, input: &str) -> Result<Value, Error> {
    // Safety checks: if either value is nil, return the other
    if !is_valid_value(l) {
        return Ok(r);
    }
    if !is_valid_value(r) {
        return Ok(l);
    }

    // Get class types
    let hash_class = ruby.class_hash();
    let array_class = ruby.class_array();

    let l_is_hash = l.is_kind_of(hash_class);
    let r_is_hash = r.is_kind_of(hash_class);
    let l_is_str = is_slice_or_string(l, ruby);
    let r_is_str = is_slice_or_string(r, ruby);
    let l_is_array = l.is_kind_of(array_class);
    let r_is_array = r.is_kind_of(array_class);

    // Two hashes: merge them
    if l_is_hash && r_is_hash {
        return l.funcall("merge", (r,));
    }

    // Two strings/slices: concatenate
    if l_is_str && r_is_str {
        if is_slice(l, ruby) {
            let l_content = slice_content(l);
            let r_content = slice_content(r);
            match slice_offset(l) {
                Ok(offset) => return create_slice(ruby, offset, &(l_content + &r_content), input),
                Err(_) => {
                    return Ok(ruby.str_new(&(l_content + &r_content)).as_value());
                }
            }
        } else if is_slice(r, ruby) {
            return Ok(r);
        } else {
            let l_content = slice_content(l);
            let r_content = slice_content(r);
            return Ok(ruby.str_new(&(l_content + &r_content)).as_value());
        }
    }

    // Hash + string: keep hash, discard string
    if l_is_hash && r_is_str {
        return Ok(l);
    }
    if r_is_hash && l_is_str {
        return Ok(r);
    }

    // Array + hash: append hash to array
    if l_is_array && r_is_hash {
        if let Some(ary) = RArray::from_value(l) {
            ary.push(r)?;
            return Ok(ary.as_value());
        }
    }

    // Hash + array: prepend hash to array
    if r_is_array && l_is_hash {
        if let Some(ary) = RArray::from_value(r) {
            ary.unshift(l)?;
            return Ok(ary.as_value());
        }
    }

    // Otherwise: create array
    let ary = ruby.ary_new();
    ary.push(l)?;
    ary.push(r)?;
    Ok(ary.as_value())
}

/// Fold an array of values using merge_fold (like Ruby's flatten_sequence)
fn fold_sequence_from_array(ary: &RArray, ruby: &Ruby, input: &str) -> Result<Value, Error> {
    let len = ary.len();
    if len == 0 {
        return Ok(ruby.qnil().as_value());
    }

    // Filter out nil values into a new array (keeps them rooted)
    let non_nil = ruby.ary_new_capa(len as _);
    for i in 0..len {
        if let Ok(item) = ary.entry::<Value>(i as isize) {
            if !item.is_nil() {
                non_nil.push(item)?;
            }
        }
    }

    let non_nil_len = non_nil.len();
    if non_nil_len == 0 {
        return Ok(ruby.qnil().as_value());
    }

    // Single item: return directly
    if non_nil_len == 1 {
        return non_nil.entry::<Value>(0);
    }

    // Check if all items are hashes
    let hash_class = ruby.class_hash();
    let mut all_hashes = true;
    for i in 0..non_nil_len {
        if let Ok(item) = non_nil.entry::<Value>(i as isize) {
            if !item.is_kind_of(hash_class) {
                all_hashes = false;
                break;
            }
        }
    }

    if all_hashes {
        // Collect all keys to check for duplicates
        let mut all_keys: Vec<String> = Vec::new();
        for i in 0..non_nil_len {
            if let Ok(item) = non_nil.entry::<Value>(i as isize) {
                if let Ok(keys) = item.funcall::<_, _, Value>("keys", ()) {
                    if let Some(keys_ary) = RArray::from_value(keys) {
                        for j in 0..keys_ary.len() {
                            if let Ok(key) = keys_ary.entry::<Value>(j as isize) {
                                if !key.is_nil() {
                                    if let Ok(key_str) = key.funcall::<_, _, String>("to_s", ()) {
                                        all_keys.push(key_str);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check for duplicate keys
        let unique_len = all_keys.iter().collect::<std::collections::HashSet<_>>().len();
        if unique_len < all_keys.len() {
            // Duplicate keys - return as array (repetition pattern)
            return Ok(non_nil.as_value());
        }

        // Check if all hashes have the same single key (wrapper pattern)
        if let Ok(first) = non_nil.entry::<Value>(0) {
            if let Ok(first_keys) = first.funcall::<_, _, Value>("keys", ()) {
                if let Some(first_keys_ary) = RArray::from_value(first_keys) {
                    if first_keys_ary.len() == 1 {
                        if let Ok(first_key) = first_keys_ary.entry::<Value>(0) {
                            if let Ok(first_key_str) = first_key.funcall::<_, _, String>("to_s", ()) {
                                // Check if all items have the same single key
                                let mut all_same = true;
                                for i in 1..non_nil_len {
                                    if let Ok(item) = non_nil.entry::<Value>(i as isize) {
                                        if let Ok(keys) = item.funcall::<_, _, Value>("keys", ()) {
                                            if let Some(keys_ary) = RArray::from_value(keys) {
                                                if keys_ary.len() != 1 {
                                                    all_same = false;
                                                    break;
                                                }
                                                if let Ok(key) = keys_ary.entry::<Value>(0) {
                                                    if let Ok(key_str) = key.funcall::<_, _, String>("to_s", ()) {
                                                        if key_str != first_key_str {
                                                            all_same = false;
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if all_same {
                                    return Ok(non_nil.as_value());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fold left using merge_fold with explicit rooting of intermediate results
    // We keep the intermediate result in a 1-element RArray to ensure it's always
    // visible to Ruby's GC, even during recursive merge operations
    let result_box = ruby.ary_new_capa(1);
    result_box.push(non_nil.entry::<Value>(0)?)?;

    for i in 1..non_nil_len {
        let current = result_box.entry::<Value>(0)?;
        let next_item = non_nil.entry::<Value>(i as isize)?;
        let merged = merge_fold(current, next_item, ruby, input)?;
        // Replace the element to keep merged value rooted
        let _: Value = result_box.pop()?;
        result_box.push(merged)?;
    }

    result_box.entry::<Value>(0)
}

/// Transform AstNode to Ruby format with recursion depth protection
fn transform_ast_internal(
    node: &AstNode,
    arena: &AstArena,
    input: &str,
    ruby: &Ruby,
    depth: u32,
) -> Result<Value, Error> {
    // Check recursion depth
    if depth > MAX_RECURSION_DEPTH {
        return Err(Error::new(
            ruby.exception_runtime_error(),
            format!("Maximum recursion depth ({}) exceeded", MAX_RECURSION_DEPTH),
        ));
    }

    match node {
        AstNode::Nil => Ok(ruby.qnil().as_value()),
        AstNode::Bool(b) => Ok((*b).into_value_with(ruby)),
        AstNode::Int(n) => Ok(ruby.integer_from_i64(*n).as_value()),
        AstNode::Float(f) => Ok(ruby.float_from_f64(*f).as_value()),
        AstNode::StringRef { pool_index } => {
            let (s, _, _) = arena.get_string_parts(*pool_index as usize);
            if s.starts_with(':') {
                return Ok(ruby.to_symbol(&s[1..]).as_value());
            }
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

            // Check for tags
            if let Some(AstNode::StringRef { pool_index: tag_idx }) = items.first() {
                let (tag, _, _) = arena.get_string_parts(*tag_idx as usize);
                if tag == ":sequence" || tag == ":repetition" || tag == ":maybe" {
                    let ary = ruby.ary_new_capa(items.len() as _);
                    let tag_sym = ruby.to_symbol(&tag[1..]);
                    ary.push(tag_sym)?;
                    for item in items.iter().skip(1) {
                        let ruby_item = transform_ast_internal(&item, arena, input, ruby, depth + 1)?;
                        ary.push(ruby_item)?;
                    }
                    return Ok(ary.as_value());
                }
            }

            // Transform items into an RArray (keeps them rooted)
            let transformed = ruby.ary_new_capa(items.len() as _);
            for item in items.iter() {
                let ruby_item = transform_ast_internal(item, arena, input, ruby, depth + 1)?;
                transformed.push(ruby_item)?;
            }

            // Check if all string-like
            if all_string_like(&transformed, ruby) {
                return join_slices_from_array(&transformed, ruby, input);
            }

            // Fold the sequence
            fold_sequence_from_array(&transformed, ruby, input)
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            let hash = ruby.hash_new();

            for (key, value) in pairs {
                if key == "spaces" {
                    if let AstNode::Array { length: 0, .. } = value {
                        continue;
                    }
                }

                let sym_key = ruby.to_symbol(&key);
                let ruby_value = transform_ast_internal(&value, arena, input, ruby, depth + 1)?;

                // Skip empty arrays
                if ruby_value.is_kind_of(ruby.class_array()) {
                    if let Ok(len) = ruby_value.funcall::<_, _, i64>("length", ()) {
                        if len == 0 {
                            continue;
                        }
                    }
                }

                hash.aset(sym_key, ruby_value)?;
            }

            Ok(hash.as_value())
        }
    }
}

/// Transform AstNode to Ruby format (public entry point)
pub fn transform_ast(
    node: &AstNode,
    arena: &AstArena,
    input: &str,
    ruby: &Ruby,
) -> Result<Value, Error> {
    transform_ast_internal(node, arena, input, ruby, 0)
}
