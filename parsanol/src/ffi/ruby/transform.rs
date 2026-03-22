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

/// Deep merge two hashes, recursively merging nested hashes
/// For keys that exist in both hashes with hash values, merge the nested hashes
/// For all other cases, right-hand value wins
#[allow(clippy::only_used_in_recursion)]
fn deep_merge_hashes(l: Value, r: Value, ruby: &Ruby, input: &str) -> Result<Value, Error> {
    let hash_class = ruby.class_hash();

    // Start with a copy of left hash
    let result = l.funcall::<_, _, Value>("dup", ())?;

    // Iterate over right hash keys
    if let Ok(r_keys) = r.funcall::<_, _, Value>("keys", ()) {
        if let Some(r_keys_ary) = RArray::from_value(r_keys) {
            for i in 0..r_keys_ary.len() {
                if let Ok(r_key) = r_keys_ary.entry::<Value>(i as isize) {
                    // Get values from both hashes
                    let l_val: Result<Value, _> = l.funcall("[]", (r_key,));
                    let r_val: Result<Value, _> = r.funcall("[]", (r_key,));

                    match (l_val, r_val) {
                        (Ok(l_v), Ok(r_v)) => {
                            let l_is_hash = l_v.is_kind_of(hash_class);
                            let r_is_hash = r_v.is_kind_of(hash_class);

                            if l_is_hash && r_is_hash {
                                // Both are hashes - recursively merge
                                let merged = deep_merge_hashes(l_v, r_v, ruby, input)?;
                                let _: Value = result.funcall("[]=", (r_key, merged))?;
                            } else {
                                // Not both hashes - right wins
                                let _: Value = result.funcall("[]=", (r_key, r_v))?;
                            }
                        }
                        (Err(_), Ok(r_v)) => {
                            // l_val is Err, but r_val is Ok - set from right
                            let _: Value = result.funcall("[]=", (r_key, r_v))?;
                        }
                        (_, Err(_)) => {
                            // r_val is Err - skip this key
                        }
                    }
                }
            }
        }
    }

    Ok(result.as_value())
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

    // Two hashes: merge them with deep merge for shared keys with nested hashes
    if l_is_hash && r_is_hash {
        // Use deep_merge! for nested hash merging
        return deep_merge_hashes(l, r, ruby, input);
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

/// Get the keys from the value at a given hash key
/// Returns Some(set of keys) if the value is a Hash, None otherwise
fn get_hash_keys(
    hash: &Value,
    outer_key: Value,
    ruby: &Ruby,
) -> Option<std::collections::HashSet<String>> {
    let hash_class = ruby.class_hash();

    if !hash.is_kind_of(hash_class) {
        return None;
    }

    let inner_value: Result<Value, _> = hash.funcall("[]", (outer_key,));
    if let Ok(inner) = inner_value {
        // Named atoms are not hashes - return None so caller handles them
        let name_var: Result<Value, _> = inner.funcall(
            "instance_variable_get",
            (ruby.to_symbol("@name").as_value(),),
        );
        if let Ok(name_var) = name_var {
            if !name_var.is_nil() {
                return None;
            }
        }

        if !inner.is_kind_of(hash_class) {
            return None;
        }

        if let Ok(keys) = inner.funcall::<_, _, Value>("keys", ()) {
            if let Some(keys_ary) = RArray::from_value(keys) {
                let len = keys_ary.len();
                let mut inner_keys: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                for i in 0..len {
                    if let Ok(k) = keys_ary.entry::<Value>(i as isize) {
                        if let Ok(key_str) = k.funcall::<_, _, String>("to_s", ()) {
                            inner_keys.insert(key_str);
                        }
                    }
                }
                return Some(inner_keys);
            }
        }
    }

    None
}

/// Get the keys from the value at a given hash key (regardless of value type)
/// Returns Some(set of keys) if the value responds to :keys with a non-nil result, None otherwise
fn get_value_hash_keys(
    hash: &Value,
    outer_key: Value,
    ruby: &Ruby,
) -> Option<std::collections::HashSet<String>> {
    if !hash.is_kind_of(ruby.class_hash()) {
        return None;
    }

    let inner_value: Result<Value, _> = hash.funcall("[]", (outer_key,));
    if let Ok(inner) = inner_value {
        // Named atoms are not hash-like - return None so this falls through to merge
        let name_var: Result<Value, _> = inner.funcall(
            "instance_variable_get",
            (ruby.to_symbol("@name").as_value(),),
        );
        if let Ok(name_var) = name_var {
            if !name_var.is_nil() {
                return None;
            }
        }

        // Check if the inner value is a Hash
        if !inner.is_kind_of(ruby.class_hash()) {
            return None;
        }

        // Inner is a Hash - get its keys
        if let Ok(keys) = inner.funcall::<_, _, Value>("keys", ()) {
            if keys.is_nil() {
                return None;
            }
            if let Some(keys_ary) = RArray::from_value(keys) {
                let len = keys_ary.len();
                let mut inner_keys: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                for i in 0..len {
                    if let Ok(k) = keys_ary.entry::<Value>(i as isize) {
                        if let Ok(key_str) = k.funcall::<_, _, String>("to_s", ()) {
                            inner_keys.insert(key_str);
                        }
                    }
                }
                return Some(inner_keys);
            }
        }
    }

    None
}

/// Get the keys from the inner value of a hash
/// Returns Some(set of inner keys) if the inner value is a hash, None otherwise
fn get_inner_keys(
    hash: &Value,
    outer_key: Value,
    ruby: &Ruby,
) -> Option<std::collections::HashSet<String>> {
    get_hash_keys(hash, outer_key, ruby)
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

    // Process based on whether all items are hashes
    if all_hashes {
        fold_hash_array(&non_nil, ruby, input)
    } else {
        // Not all items are hashes - separate hash from non-hash items
        let hash_items = ruby.ary_new_capa(non_nil_len as _);
        let non_hash_items = ruby.ary_new_capa(non_nil_len as _);
        for i in 0..non_nil_len {
            if let Ok(item) = non_nil.entry::<Value>(i as isize) {
                if item.is_kind_of(hash_class) {
                    hash_items.push(item)?;
                } else {
                    non_hash_items.push(item)?;
                }
            }
        }

        let hash_len = hash_items.len();
        let non_hash_len = non_hash_items.len();

        // Process hash items with wrapper/repetition logic
        let hash_result = if hash_len > 0 {
            fold_hash_array(&hash_items, ruby, input)?
        } else {
            ruby.qnil().as_value()
        };

        if non_hash_len == 0 {
            // No non-hash items, return hash result directly
            return Ok(hash_result);
        }

        // Combine hash result with non-hash items using merge_fold
        let result_box = ruby.ary_new_capa(1);
        result_box.push(hash_result)?;

        for i in 0..non_hash_len {
            if let Ok(item) = non_hash_items.entry::<Value>(i as isize) {
                let current = result_box.entry::<Value>(0)?;
                let merged = merge_fold(current, item, ruby, input)?;
                let _: Value = result_box.pop()?;
                result_box.push(merged)?;
            }
        }

        result_box.entry::<Value>(0)
    }
}

/// Fold an array of hash values using merge_fold
/// Handles wrapper vs repetition pattern detection
fn fold_hash_array(ary: &RArray, ruby: &Ruby, _input: &str) -> Result<Value, Error> {
    let non_nil_len = ary.len();
    if non_nil_len == 0 {
        return Ok(ruby.qnil().as_value());
    }
    if non_nil_len == 1 {
        return ary.entry::<Value>(0);
    }

    // Collect all keys to check for duplicates
    let mut all_keys: Vec<String> = Vec::new();
    for i in 0..non_nil_len {
        if let Ok(item) = ary.entry::<Value>(i as isize) {
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

    let unique_len = all_keys
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    let has_duplicates = unique_len < all_keys.len();

    // Check if all hashes have the same single key
    let mut all_same_single_key = true;
    let mut first_key_str_opt: Option<String> = None;

    if let Ok(first) = ary.entry::<Value>(0) {
        if let Ok(first_keys) = first.funcall::<_, _, Value>("keys", ()) {
            if let Some(first_keys_ary) = RArray::from_value(first_keys) {
                if first_keys_ary.len() == 1 {
                    if let Ok(first_key) = first_keys_ary.entry::<Value>(0) {
                        if let Ok(first_key_str) = first_key.funcall::<_, _, String>("to_s", ()) {
                            first_key_str_opt = Some(first_key_str.clone());
                            // Check if all items have the same single key
                            for i in 1..non_nil_len {
                                if let Ok(item) = ary.entry::<Value>(i as isize) {
                                    if let Ok(keys) = item.funcall::<_, _, Value>("keys", ()) {
                                        if let Some(keys_ary) = RArray::from_value(keys) {
                                            if keys_ary.len() != 1 {
                                                all_same_single_key = false;
                                                break;
                                            }
                                            if let Ok(key) = keys_ary.entry::<Value>(0) {
                                                if let Ok(key_str) =
                                                    key.funcall::<_, _, String>("to_s", ())
                                                {
                                                    if key_str != first_key_str {
                                                        all_same_single_key = false;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    all_same_single_key = false;
                }
            }
        }
    }

    // If all items have same single outer key, check inner keys to determine wrapper vs repetition
    if all_same_single_key {
        if let Some(ref first_key_str) = first_key_str_opt {
            let first_key_sym = ruby.to_symbol(first_key_str).as_value();
            let first_inner_keys =
                get_inner_keys(&ary.entry::<Value>(0).unwrap(), first_key_sym, ruby);

            if let Some(inner_keys_set) = first_inner_keys {
                // Inner value is a hash - check if all have same inner keys
                let mut all_same_inner = true;
                for i in 1..non_nil_len {
                    if let Ok(item) = ary.entry::<Value>(i as isize) {
                        if let Some(item_inner_keys) = get_inner_keys(&item, first_key_sym, ruby) {
                            if item_inner_keys != inner_keys_set {
                                all_same_inner = false;
                                break;
                            }
                        } else {
                            // Inner value is not a hash
                            all_same_inner = false;
                            break;
                        }
                    }
                }

                if all_same_inner {
                    // REPETITION pattern: same inner keys
                    // Keep as array of hashes
                    return Ok(ary.as_value());
                }

                // DIFFERENT INNER KEYS pattern: same outer key with different inner keys
                // This is a WRAPPER pattern - keep all items as array
                // Example: [{:syntax => {:spaces => ...}}, {:syntax => {:schemaDecl => [...]}}]
                // Should NOT merge or drop items - keep all declarations
                return Ok(ary.as_value());
            } else {
                // Inner value is NOT a hash
                // Check if all inner values have the same hash-like structure
                let first_hash_keys =
                    get_hash_keys(&ary.entry::<Value>(0).unwrap(), first_key_sym, ruby);
                if let Some(first_keys_set) = first_hash_keys {
                    let mut all_same_structure = true;
                    for i in 1..non_nil_len {
                        if let Ok(item) = ary.entry::<Value>(i as isize) {
                            if let Some(item_keys) = get_hash_keys(&item, first_key_sym, ruby) {
                                if item_keys != first_keys_set {
                                    all_same_structure = false;
                                    break;
                                }
                            } else {
                                all_same_structure = false;
                                break;
                            }
                        }
                    }

                    if all_same_structure {
                        // REPETITION pattern: same hash structure
                        return Ok(ary.as_value());
                    }
                    // else: DUPLICATE KEY pattern - fall through
                } else {
                    // Inner values are not hashes
                    let first_val_keys =
                        get_value_hash_keys(&ary.entry::<Value>(0).unwrap(), first_key_sym, ruby);
                    if let Some(first_val_set) = first_val_keys {
                        let mut all_same_structure = true;
                        for i in 1..non_nil_len {
                            if let Ok(item) = ary.entry::<Value>(i as isize) {
                                if let Some(item_keys) =
                                    get_value_hash_keys(&item, first_key_sym, ruby)
                                {
                                    if item_keys != first_val_set {
                                        all_same_structure = false;
                                        break;
                                    }
                                } else {
                                    all_same_structure = false;
                                    break;
                                }
                            }
                        }

                        if all_same_structure {
                            // REPETITION pattern: same non-hash structure
                            return Ok(ary.as_value());
                        }
                        // else: DUPLICATE KEY pattern - fall through
                    } else {
                        // REPETITION pattern: non-hash values (Slices, strings)
                        return Ok(ary.as_value());
                    }
                }
            }
        }
    } else if has_duplicates {
        // Different outer keys with duplicates
        return Ok(ary.as_value());
    }

    // Fall through to merge_fold (left-fold with deep merge)
    let result_box = ruby.ary_new_capa(1);
    result_box.push(ary.entry::<Value>(0)?)?;

    for i in 1..non_nil_len {
        let current = result_box.entry::<Value>(0)?;
        let next_item = ary.entry::<Value>(i as isize)?;
        let merged = merge_fold(current, next_item, ruby, "")?;
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
            let (s, _, _, _) = arena.get_string_parts(*pool_index as usize);
            if let Some(stripped) = s.strip_prefix(':') {
                return Ok(ruby.to_symbol(stripped).as_value());
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
            if let Some(AstNode::StringRef {
                pool_index: tag_idx,
            }) = items.first()
            {
                let (tag, _, _, _) = arena.get_string_parts(*tag_idx as usize);
                if tag == ":sequence" || tag == ":repetition" || tag == ":maybe" {
                    let ary = ruby.ary_new_capa(items.len() as _);
                    let tag_sym = ruby.to_symbol(&tag[1..]);
                    ary.push(tag_sym)?;
                    for item in items.iter().skip(1) {
                        let ruby_item =
                            transform_ast_internal(item, arena, input, ruby, depth + 1)?;
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

        AstNode::Tagged { tag: _, value } => {
            // Tagged nodes should have been processed by to_parslet_compatible already
            // For safety, just transform the inner value
            transform_ast_internal(value, arena, input, ruby, depth)
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
