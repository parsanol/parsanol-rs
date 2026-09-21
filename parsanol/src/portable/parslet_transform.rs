//! Parslet-compatible AST transformation
//!
//! This module provides transformation from raw parse trees to Parslet-compatible
//! format. It implements the same sequence flattening semantics as Parslet, making
//! the output consistent across Rust, Ruby, and other language bindings.
//!
//! # What is Parslet Transformation?
//!
//! When parsing sequences with named captures, Parslet applies special semantics:
//!
//! ```text
//! // Grammar: str("SCHEMA ") >> match("[a-z]+").label("name") >> str(";")
//! // Input: "SCHEMA test;"
//!
//! // Raw AST (before transformation):
//! ["SCHEMA ", {:name => "test"}, ";"]
//!
//! // Parslet-compatible AST (after transformation):
//! {:name => "test"}
//! ```
//!
//! The transformation:
//! 1. Merges all named captures into a single hash
//! 2. Discards unnamed strings (when named captures are present)
//! 3. Handles repetition vs wrapper patterns correctly
//!
//! # Repetition vs Wrapper Patterns
//!
//! The transformation distinguishes between two common patterns:
//!
//! ## Repetition Pattern
//!
//! ```text
//! // Grammar: match("[a-z]").label("letter").repeat(1, None)
//! // Input: "abc"
//!
//! // Raw: [{:letter => "a"}, {:letter => "b"}, {:letter => "c"}]
//! // Transformed: [{:letter => "a"}, {:letter => "b"}, {:letter => "c"}]
//! ```
//!
//! Values are SIMPLE (strings), so keep as array.
//!
//! ## Wrapper Pattern
//!
//! ```text
//! // Grammar: expr.label("syntax") >> stmt.label("syntax")
//! // Input: "foo bar"
//!
//! // Raw: [{:syntax => {:expr => "foo"}}, {:syntax => {:stmt => "bar"}}]
//! // Transformed: {:syntax => {:expr => "foo", :stmt => "bar"}}
//! ```
//!
//! Values are HASHES with DIFFERENT inner keys, so merge under wrapper key.
//!
//! # Usage
//!
//! ```rust,ignore
//! use parsanol::portable::{Grammar, AstArena};
//! use parsanol::portable::parser_dsl::{GrammarBuilder, str, re};
//! use parsanol::portable::parslet_transform::to_parslet_compatible;
//!
//! // Build grammar
//! let grammar = GrammarBuilder::new()
//!     .rule("schema", str("SCHEMA ") >> re("[a-z]+").label("name") >> str(";"))
//!     .build();
//!
//! // Parse with arena
//! let input = "SCHEMA test;";
//! let mut arena = AstArena::for_input(input.len());
//! let mut parser = parsanol::portable::parser::PortableParser::new(&grammar, input, &mut arena);
//!
//! // Parse raw AST
//! let raw_ast = parser.parse().unwrap();
//!
//! // Transform to Parslet-compatible format
//! let parslet_ast = to_parslet_compatible(&raw_ast, &mut arena, input);
//!
//! // parslet_ast is now: {:name => "test"}
//! ```

use std::collections::HashMap;

use super::arena::AstArena;
use super::ast::AstNode;

/// Transform a raw AST to Parslet-compatible format
///
/// This function applies Parslet's sequence flattening semantics to produce
/// a more idiomatic AST structure.
///
/// # Arguments
///
/// * `node` - The root AST node to transform
/// * `arena` - The arena containing the AST data (modified to store transformed nodes)
/// * `input` - The original input string (for string references)
///
/// # Returns
///
/// A new AST node with Parslet-compatible structure.
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::portable::parslet_transform::to_parslet_compatible;
///
/// let raw = parser.parse("SCHEMA test;")?;
/// let transformed = to_parslet_compatible(&raw, &mut arena, input);
/// // transformed: {:name => "test"} instead of ["SCHEMA ", {:name => "test"}, ";"]
/// ```
pub fn to_parslet_compatible(node: &AstNode, arena: &mut AstArena, input: &str) -> AstNode {
    match node {
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);

            // Only the FIRST element can be a tag (":sequence",
            // ":repetition", ":maybe"). Stripping ':'-prefixed strings
            // anywhere else would eat literal colons from the input,
            // which the Ruby transformer never does.
            let (tag_kind, content): (Option<String>, Vec<AstNode>) = match items.split_first() {
                Some((first, rest)) if is_tag_node(first, arena) => {
                    (Some(tag_text(first, arena)), rest.to_vec())
                }
                _ => (None, items.clone()),
            };

            match tag_kind.as_deref() {
                // A maybe flattens to its single value, never to an array.
                Some(":maybe") => {
                    if content.is_empty() {
                        return arena.intern_string("");
                    }
                    return to_parslet_compatible(&content[0], arena, input);
                }
                // Repetitions keep their items (joined only when every
                // item is a string); they never merge named captures.
                Some(":repetition") => {
                    let transformed_items: Vec<AstNode> = content
                        .iter()
                        .map(|item| to_parslet_compatible(item, arena, input))
                        .collect();
                    return flatten_repetition(&transformed_items, arena, input);
                }
                // An empty :sequence matched no content and flattens to
                // "" (Ruby semantics; e.g. a labeled sequence of optional
                // clauses with none present).
                Some(tag) if tag != ":repetition" && tag != ":maybe" && content.is_empty() => {
                    return arena.intern_string("");
                }
                Some(_) | None => {}
            }

            let transformed_items: Vec<AstNode> = content
                .iter()
                .map(|item| to_parslet_compatible(item, arena, input))
                .collect();
            flatten_sequence(&transformed_items, arena, input)
        }
        AstNode::Hash { pool_index, length } => {
            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
            if pairs.len() == 1 {
                transform_single_key_hash(&pairs[0], arena, input)
            } else {
                transform_multi_key_hash(&pairs, arena, input)
            }
        }
        other => other.clone(),
    }
}

/// Check if a node is a tag (StringRef or InputRef pointing to string starting with ':')
fn is_tag_node(node: &AstNode, arena: &AstArena) -> bool {
    match node {
        AstNode::StringRef { pool_index } => {
            let (s, _, _, _) = arena.get_string_parts(*pool_index as usize);
            s.starts_with(':')
        }
        AstNode::InputRef { offset, length } => {
            // Get string from input and check if it's a tag
            let start = *offset as usize;
            let end = start + *length as usize;
            if let Some(s) = arena.get_input().get(start..end) {
                s.starts_with(':')
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Text of a tag node (the tag string itself).
fn tag_text(node: &AstNode, arena: &AstArena) -> String {
    match node {
        AstNode::StringRef { pool_index } => arena.get_string(*pool_index as usize).to_string(),
        AstNode::InputRef { offset, length } => arena
            .get_input()
            .get(*offset as usize..(*offset + *length) as usize)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// Array node whose first element is the given tag.
fn is_tagged_with(node: &AstNode, tag: &str, arena: &AstArena) -> bool {
    match node {
        AstNode::Array { pool_index, length } if *length > 0 => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            items
                .first()
                .is_some_and(|first| is_tag_node(first, arena) && tag_text(first, arena) == tag)
        }
        _ => false,
    }
}

/// Empty string node (zero-length input ref or empty interned string).
fn is_empty_string_node(node: &AstNode, arena: &AstArena) -> bool {
    match node {
        AstNode::InputRef { length, .. } => *length == 0,
        AstNode::StringRef { pool_index } => arena.get_string(*pool_index as usize).is_empty(),
        _ => false,
    }
}

/// Items of an array node, when the node is an array.
fn array_items_of(node: &AstNode, arena: &AstArena) -> Option<Vec<AstNode>> {
    match node {
        AstNode::Array { pool_index, length } => {
            Some(arena.get_array(*pool_index as usize, *length as usize))
        }
        _ => None,
    }
}

/// All items are single-key hashes carrying `key`.
fn all_items_hash_with_key(items: &[AstNode], key: &str, arena: &AstArena) -> bool {
    !items.is_empty() && items.iter().all(|item| is_hash_with_key(item, key, arena))
}

/// Repetition flattening (port of the Ruby transformer's
/// flatten_repetition): flatten one level of nested arrays, join when
/// every remaining item is a string, otherwise keep the item array.
fn flatten_repetition(items: &[AstNode], arena: &mut AstArena, input: &str) -> AstNode {
    let mut flat: Vec<AstNode> = Vec::with_capacity(items.len());
    for item in items {
        match item {
            AstNode::Array { pool_index, length } => {
                flat.extend(arena.get_array(*pool_index as usize, *length as usize));
            }
            other => flat.push(other.clone()),
        }
    }
    if flat.is_empty() {
        return store_array_node(&[], arena);
    }
    let mut string_parts: Vec<String> = Vec::with_capacity(flat.len());
    let mut first_input_offset: Option<u32> = None;
    for item in &flat {
        match item {
            AstNode::InputRef { offset, length } => {
                if first_input_offset.is_none() {
                    first_input_offset = Some(*offset);
                }
                if let Some(s) = input.get(*offset as usize..(*offset + *length) as usize) {
                    string_parts.push(s.to_string());
                }
            }
            AstNode::StringRef { pool_index } => {
                let (s, _, _, _) = arena.get_string_parts(*pool_index as usize);
                if !s.starts_with(':') {
                    string_parts.push(s.to_string());
                }
            }
            _ => return store_array_node(&flat, arena),
        }
    }
    join_string_parts(&string_parts, first_input_offset, arena)
}

fn store_array_node(items: &[AstNode], arena: &mut AstArena) -> AstNode {
    let (pool_index, length) = arena.store_array(items);
    AstNode::Array { pool_index, length }
}

fn join_string_parts(
    string_parts: &[String],
    first_input_offset: Option<u32>,
    arena: &mut AstArena,
) -> AstNode {
    if string_parts.len() == 1 {
        if let Some(offset) = first_input_offset {
            return arena.intern_string_with_offset(&string_parts[0], offset);
        }
        return arena.intern_string(&string_parts[0]);
    }
    let joined: String = string_parts.concat();
    let offset = first_input_offset.unwrap_or(0);
    arena.intern_string_with_offset(&joined, offset)
}

/// Transform a single-key hash (the common case)
///
/// Single-key hashes are produced by named captures like `.label("name")`.
/// Ports the Ruby transformer's transform_single_key_hash, including its
/// repetition detection: a repetition value keeps its items (each
/// non-hash item re-wrapped under the key) instead of merging.
fn transform_single_key_hash(
    pair: &(String, AstNode),
    arena: &mut AstArena,
    input: &str,
) -> AstNode {
    let (key, value) = pair;
    let key_str = key.as_str();
    let transformed = to_parslet_compatible(value, arena, input);

    let is_tagged_repetition = is_tagged_with(value, ":repetition", arena);
    let is_raw_array_repetition = array_items_of(value, arena)
        .map(|items| {
            // Exclude the tag itself when checking raw items.
            let content: Vec<AstNode> = items
                .into_iter()
                .filter(|i| !is_tag_node(i, arena))
                .collect();
            all_items_hash_with_key(&content, key_str, arena)
        })
        .unwrap_or(false);
    let is_empty_repetition = matches!(value, AstNode::Array { length: 0, .. });
    let is_transformed_repetition = array_items_of(&transformed, arena)
        .map(|items| all_items_hash_with_key(&items, key_str, arena))
        .unwrap_or(false);
    let is_repetition = is_tagged_repetition
        || is_raw_array_repetition
        || is_transformed_repetition
        || is_empty_repetition;

    if is_repetition {
        transform_repetition_value(key_str, transformed, arena)
    } else if let AstNode::Array { .. } = transformed {
        transform_array_value(key_str, &transformed, arena)
    } else {
        wrap_with_key(key_str, transformed, arena)
    }
}

fn wrap_with_key(key: &str, value: AstNode, arena: &mut AstArena) -> AstNode {
    let (pool_idx, len) = arena.store_hash(&[(key, value)]);
    AstNode::Hash {
        pool_index: pool_idx,
        length: len,
    }
}

/// Port of the Ruby transformer's transform_repetition_value.
fn transform_repetition_value(key: &str, transformed: AstNode, arena: &mut AstArena) -> AstNode {
    let result = match &transformed {
        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            if items.is_empty() {
                store_array_node(&[], arena)
            } else if items.iter().all(|i| matches!(i, AstNode::Hash { .. })) {
                transformed.clone()
            } else {
                let rewrapped: Vec<AstNode> = items
                    .iter()
                    .map(|item| wrap_with_key(key, item.clone(), arena))
                    .collect();
                store_array_node(&rewrapped, arena)
            }
        }
        other if is_empty_string_node(other, arena) => store_array_node(&[], arena),
        other => other.clone(),
    };
    wrap_with_key(key, result, arena)
}

/// Port of the Ruby transformer's transform_array_value (non-repetition
/// arrays): empty arrays become an empty string, anything else is kept.
fn transform_array_value(key: &str, transformed: &AstNode, arena: &mut AstArena) -> AstNode {
    let value = if let AstNode::Array { length: 0, .. } = transformed {
        arena.intern_string("")
    } else {
        transformed.clone()
    };
    wrap_with_key(key, value, arena)
}

/// Transform a multi-key hash (rare case)
fn transform_multi_key_hash(
    pairs: &[(String, AstNode)],
    arena: &mut AstArena,
    input: &str,
) -> AstNode {
    // Transform values first, then collect references
    let transformed_owned: Vec<(String, AstNode)> = pairs
        .iter()
        .map(|(k, v)| (k.clone(), to_parslet_compatible(v, arena, input)))
        .collect();

    let transformed_refs: Vec<(&str, AstNode)> = transformed_owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();

    let (pool_idx, len) = arena.store_hash(&transformed_refs);
    AstNode::Hash {
        pool_index: pool_idx,
        length: len,
    }
}

/// Check if a node is a hash with a specific key
fn is_hash_with_key(node: &AstNode, key: &str, arena: &AstArena) -> bool {
    if let AstNode::Hash { pool_index, length } = node {
        let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
        pairs.len() == 1 && pairs[0].0 == key
    } else {
        false
    }
}

/// Flatten a sequence according to Parslet semantics
///
/// This is the core transformation logic that implements:
/// 1. Merge named captures into single hash
/// 2. Discard unnamed strings when named captures present
/// 3. Handle repetition vs wrapper patterns
fn flatten_sequence(items: &[AstNode], arena: &mut AstArena, input: &str) -> AstNode {
    if items.is_empty() {
        return AstNode::Array {
            pool_index: 0,
            length: 0,
        };
    }

    // Ruby CanFlatten#flatten_sequence is a left fold of merge_fold.
    // Critically, Hash+Array hoists (`[hash] + array`) rather than
    // splicing the array first — that distinction is what keeps
    // `item >> (sep >> item).repeat` as a list when `sep` is a named
    // capture (EXPRESS `op_comma`, `op_delim`). Splicing first turns
    // those into Hash+Hash pairs that last-wins-merge and drop every
    // element but the last (the multi-parameter/attribute bug).
    //
    // Array+"" / Array+Nil keep the array (#83): an empty-maybe's ""
    // behaves like parslet's nil and is dropped when a structured
    // sibling is present.
    if items
        .iter()
        .any(|i| matches!(i, AstNode::Array { .. } | AstNode::Nil))
    {
        return fold_sequence_ruby(items, arena, input);
    }

    // DON'T unwrap single items - let the caller handle this
    // This preserves repetition results like [{:x => 1}]
    // The caller (transform_single_key_hash or parent sequence) will decide
    // whether to merge or keep as array based on context
    if items.len() == 1 {
        // Check if this single item is a hash (repetition result)
        // If so, return it as an array to preserve the repetition structure
        if matches!(items[0], AstNode::Hash { .. }) {
            let (pool_idx, len) = arena.store_array(items);
            return AstNode::Array {
                pool_index: pool_idx,
                length: len,
            };
        }
        // Non-hash single item: return as-is
        return items[0].clone();
    }

    // FIRST PASS: Detect repetition patterns
    // If any key appears more than once across all hashes, this is a repetition
    // pattern and we should keep items as an array instead of merging.
    let mut key_counts: HashMap<String, usize> = HashMap::new();

    fn count_keys_in_item(item: &AstNode, arena: &AstArena, counts: &mut HashMap<String, usize>) {
        match item {
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                for (k, _) in pairs {
                    *counts.entry(k.to_string()).or_insert(0) += 1;
                }
            }
            AstNode::Array { pool_index, length } => {
                let nested = arena.get_array(*pool_index as usize, *length as usize);
                for nested_item in nested {
                    count_keys_in_item(&nested_item, arena, counts);
                }
            }
            _ => {}
        }
    }

    for item in items {
        count_keys_in_item(item, arena, &mut key_counts);
    }

    // Check for repetition pattern: any key appearing more than once
    let has_repetition = key_counts.values().any(|&count| count > 1);

    if has_repetition {
        // TRUE REPETITION: repeated keys across sibling hashes.
        // Keep as array. (Named-separator list forms never reach here —
        // they take the fold_sequence_ruby path above because the
        // repetition child is still an Array.)
        // Example: [{letter: 'a'}, {letter: 'b'}] or [{schemaDecl: ...}, ...]
        let mut flat_items: Vec<AstNode> = Vec::new();
        for item in items {
            match item {
                AstNode::Array { pool_index, length } => {
                    let nested = arena.get_array(*pool_index as usize, *length as usize);
                    flat_items.extend(nested.iter().cloned());
                }
                _ => flat_items.push(item.clone()),
            }
        }
        let (pool_idx, len) = arena.store_array(&flat_items);
        return AstNode::Array {
            pool_index: pool_idx,
            length: len,
        };
    }

    // SEQUENCE PATTERN: proceed with existing merge logic
    // Second pass: collect all data without mutating arena
    // Use owned Strings for keys to avoid lifetime issues
    let mut merged_hash: Vec<(String, AstNode)> = Vec::new();
    let mut string_parts: Vec<String> = Vec::new();
    let mut first_input_offset: Option<u32> = None;
    let mut hash_count = 0;
    let mut total_items = 0;

    for item in items {
        match item {
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                for (k, v) in pairs {
                    // Check if key already exists (will be overwritten)
                    if let Some(pos) = merged_hash.iter().position(|(key, _)| *key == k) {
                        merged_hash[pos] = (k.clone(), v);
                    } else {
                        merged_hash.push((k.clone(), v));
                    }
                }
                hash_count += 1;
                total_items += 1;
            }
            AstNode::InputRef { offset, length } => {
                // Track first InputRef offset for joined strings
                if first_input_offset.is_none() {
                    first_input_offset = Some(*offset);
                }
                // Get string from input
                let start = *offset as usize;
                let end = start + *length as usize;
                if let Some(s) = input.get(start..end) {
                    string_parts.push(s.to_string());
                }
                total_items += 1;
            }
            AstNode::StringRef { pool_index } => {
                let (s, _, _, _) = arena.get_string_parts(*pool_index as usize);
                // Skip tags (strings starting with ':') - these are metadata, not content
                if !s.starts_with(':') {
                    string_parts.push(s.to_string());
                }
                total_items += 1;
            }
            AstNode::Array { pool_index, length } => {
                // Flatten nested arrays
                let nested = arena.get_array(*pool_index as usize, *length as usize);
                for nested_item in nested {
                    count_keys_in_item(&nested_item, arena, &mut key_counts);
                }
            }
            AstNode::Nil => {
                // Skip nil values (from lookahead or optional)
            }
            _ => {
                total_items += 1;
            }
        }
    }

    // KEY INSIGHT: If ALL items are hashes, determine pattern type
    if hash_count == total_items && hash_count > 1 {
        // Check if all hashes have the same single key (wrapper vs repetition)
        if let Some(first_key) = get_single_key(&items[0], arena) {
            let all_same_key = items
                .iter()
                .all(|item| get_single_key(item, arena).is_some_and(|k| k == first_key));

            if all_same_key {
                // ALL hashes have the SAME outer key -> REPETITION pattern
                // Keep items as array (do NOT merge)
                // This matches Ruby's flatten_sequence: "return items unless all_values_are_hashes"
                let (pool_idx, len) = arena.store_array(items);
                return AstNode::Array {
                    pool_index: pool_idx,
                    length: len,
                };
            }

            // DIFFERENT outer keys -> plain sequence merge of the item
            // hashes (siblings), matching Ruby's "MIXED KEYS: merge into
            // single hash". Falls through to the merged_hash return below.
        }

        // First item is not a single-key hash: also a plain sequence
        // merge in Ruby semantics — falls through to merged_hash.
    }

    // PARSLET SEQUENCE SEMANTICS:
    // If there are named captures (hashes), return ONLY the merged hash
    if !merged_hash.is_empty() {
        // Convert to borrowed slices for store_hash
        let hash_refs: Vec<(&str, AstNode)> = merged_hash
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();
        let (pool_idx, len) = arena.store_hash(&hash_refs);
        return AstNode::Hash {
            pool_index: pool_idx,
            length: len,
        };
    }

    // No named captures - handle strings
    if !string_parts.is_empty() {
        if string_parts.len() == 1 {
            // Return single string as InputRef with correct offset
            if let Some(offset) = first_input_offset {
                return arena.intern_string_with_offset(&string_parts[0], offset);
            }
            return arena.intern_string(&string_parts[0]);
        } else {
            // Join strings with the correct input offset
            let joined: String = string_parts.join("");
            let offset = first_input_offset.unwrap_or(0);
            return arena.intern_string_with_offset(&joined, offset);
        }
    }

    // Only other items
    if total_items == 0 {
        return AstNode::Array {
            pool_index: 0,
            length: 0,
        };
    }

    if items.len() == 1 {
        items[0].clone()
    } else {
        let (pool_idx, len) = arena.store_array(items);
        AstNode::Array {
            pool_index: pool_idx,
            length: len,
        }
    }
}

/// Left-fold a sequence with Ruby CanFlatten#merge_fold semantics.
///
/// Preserves the Hash↔Array distinction that splicing would destroy:
/// `item >> (sep >> item).repeat` stays a list even when `sep` is named.
fn fold_sequence_ruby(items: &[AstNode], arena: &mut AstArena, input: &str) -> AstNode {
    let mut acc: Option<AstNode> = None;
    for item in items {
        // Drop bare nils and empty arrays (absent optionals / failed
        // maybes / zero-occurrence repetitions — Ruby can_flatten
        // skips both and merges siblings). Treated as Nil above.
        if matches!(item, AstNode::Nil) {
            continue;
        }
        if let AstNode::Array {
            pool_index,
            length,
        } = item
        {
            if *length == 0 {
                continue;
            }
            // Inflate a (start, start) view so the merge_fold below
            // can match on it; cheaper than cloning the array values
            // into a new AstNode just to check.
            let _ = *pool_index;
        }
        acc = Some(match acc {
            None => item.clone(),
            Some(left) => merge_fold_ruby(left, item.clone(), arena, input),
        });
    }
    match acc {
        Some(node) => node,
        None => arena.intern_string(""),
    }
}

/// Ruby `CanFlatten#merge_fold` — equal types merge, unequal hoist.
fn merge_fold_ruby(left: AstNode, right: AstNode, arena: &mut AstArena, input: &str) -> AstNode {
    match (&left, &right) {
        // Hash + Hash → last-wins merge (duplicate .as labels in a pure sequence).
        (
            AstNode::Hash {
                pool_index: lp,
                length: ll,
            },
            AstNode::Hash {
                pool_index: rp,
                length: rl,
            },
        ) => {
            let mut merged: Vec<(String, AstNode)> = arena
                .get_hash_items(*lp as usize, *ll as usize)
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (k, v) in arena.get_hash_items(*rp as usize, *rl as usize) {
                if let Some(pos) = merged.iter().position(|(key, _)| key == &k) {
                    merged[pos] = (k, v);
                } else {
                    merged.push((k, v));
                }
            }
            let refs: Vec<(&str, AstNode)> = merged
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect();
            let (pool_idx, len) = arena.store_hash(&refs);
            AstNode::Hash {
                pool_index: pool_idx,
                length: len,
            }
        }

        // Array + Array → concat.
        (
            AstNode::Array {
                pool_index: lp,
                length: ll,
            },
            AstNode::Array {
                pool_index: rp,
                length: rl,
            },
        ) => {
            let mut out = arena
                .get_array(*lp as usize, *ll as usize)
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            out.extend(
                arena
                    .get_array(*rp as usize, *rl as usize)
                    .iter()
                    .cloned(),
            );
            let (pool_idx, len) = arena.store_array(&out);
            AstNode::Array {
                pool_index: pool_idx,
                length: len,
            }
        }

        // Hash + Array → [hash] + array  (list pattern: first item + repetition).
        (
            AstNode::Hash { .. },
            AstNode::Array {
                pool_index,
                length,
            },
        ) => {
            let mut out = vec![left];
            out.extend(
                arena
                    .get_array(*pool_index as usize, *length as usize)
                    .iter()
                    .cloned(),
            );
            let (pool_idx, len) = arena.store_array(&out);
            AstNode::Array {
                pool_index: pool_idx,
                length: len,
            }
        }

        // Array + Hash → array + [hash].
        (
            AstNode::Array {
                pool_index,
                length,
            },
            AstNode::Hash { .. },
        ) => {
            let mut out = arena
                .get_array(*pool_index as usize, *length as usize)
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            out.push(right);
            let (pool_idx, len) = arena.store_array(&out);
            AstNode::Array {
                pool_index: pool_idx,
                length: len,
            }
        }

        // Structured + stringlike → keep structured (unnamed tokens discarded
        // when named captures / arrays are present). Covers #83 empty-maybe "".
        (AstNode::Hash { .. } | AstNode::Array { .. }, other) if is_stringlike(other, arena) => {
            left
        }
        (other, AstNode::Hash { .. } | AstNode::Array { .. }) if is_stringlike(other, arena) => {
            right
        }

        // Both stringlike → concatenate.
        (l, r) if is_stringlike(l, arena) && is_stringlike(r, arena) => {
            let (ls, lo) = stringlike_text(l, arena, input);
            let (rs, _) = stringlike_text(r, arena, input);
            let joined = format!("{ls}{rs}");
            match lo {
                Some(off) => arena.intern_string_with_offset(&joined, off),
                None => arena.intern_string(&joined),
            }
        }

        // Fallback: prefer structured side, else right.
        (AstNode::Hash { .. } | AstNode::Array { .. }, _) => left,
        (_, AstNode::Hash { .. } | AstNode::Array { .. }) => right,
        _ => right,
    }
}

fn is_stringlike(node: &AstNode, arena: &AstArena) -> bool {
    match node {
        AstNode::InputRef { .. } => true,
        AstNode::StringRef { pool_index } => {
            let s = arena.get_string(*pool_index as usize);
            !s.starts_with(':') // tags are metadata, not content
        }
        _ => false,
    }
}

fn stringlike_text(node: &AstNode, arena: &AstArena, input: &str) -> (String, Option<u32>) {
    match node {
        AstNode::InputRef { offset, length } => {
            let start = *offset as usize;
            let end = start + *length as usize;
            let s = input.get(start..end).unwrap_or("").to_string();
            (s, Some(*offset))
        }
        AstNode::StringRef { pool_index } => {
            let (s, _, _, _) = arena.get_string_parts(*pool_index as usize);
            (s.to_string(), None)
        }
        _ => (String::new(), None),
    }
}

/// Get the single key from a hash node, if it has exactly one key
///
/// Returns an owned String to avoid lifetime issues with the arena's internal storage.
fn get_single_key(node: &AstNode, arena: &AstArena) -> Option<String> {
    if let AstNode::Hash { pool_index, length } = node {
        let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
        if pairs.len() == 1 {
            return Some(pairs[0].0.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::grammar::Grammar;
    use crate::portable::parser::PortableParser;
    use crate::portable::parser_dsl::{dynamic, re, ref_, seq, str, GrammarBuilder, ParsletExt};

    fn parse_and_transform(input: &str, grammar: &Grammar) -> (AstNode, AstArena) {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(grammar, input, &mut arena);
        let raw = parser.parse().unwrap();
        let transformed = to_parslet_compatible(&raw, &mut arena, input);
        (transformed, arena)
    }

    #[test]
    fn test_sequence_flattening() {
        // Grammar: str("SCHEMA ") >> re("[a-z]+").label("name") >> str(";")
        let grammar = GrammarBuilder::new()
            .rule(
                "schema",
                seq(vec![
                    dynamic(str("SCHEMA ")),
                    dynamic(re("[a-z]+").label("name")),
                    dynamic(str(";")),
                ]),
            )
            .build();

        let (result, arena) = parse_and_transform("SCHEMA test;", &grammar);

        // Should produce: {:name => "test"}
        if let AstNode::Hash { pool_index, length } = result {
            let pairs = arena.get_hash_items(pool_index as usize, length as usize);
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].0, "name");
        } else {
            panic!("Expected hash, got {:?}", result);
        }
    }

    #[test]
    fn test_repetition_pattern() {
        // Grammar: re("[a-z]").label("letter").repeat(1, None)
        // This produces: Repetition(Named("letter", Match("[a-z]")), 1, None)
        // Which parses "abc" into: [{letter: "a"}, {letter: "b"}, {letter: "c"}]
        // After transformation, this should stay as an array of hashes (repetition pattern)
        // The array is tagged with :repetition for proper transformation
        let grammar = GrammarBuilder::new()
            .rule("letters", re("[a-z]").label("letter").repeat(1, None))
            .build();

        let (result, arena) = parse_and_transform("abc", &grammar);

        // For repetition pattern with named captures BEFORE repeat,
        // the result should be an ARRAY with :repetition tag + hashes
        if let AstNode::Array { pool_index, length } = result {
            let items = arena.get_array(pool_index as usize, length as usize);
            // Array has 3 items (tag is stripped by to_parslet_compatible)
            assert_eq!(
                items.len(),
                3,
                "should have 3 hash items after tag stripping"
            );

            // All items should be hashes with key "letter" (no tag after stripping)
            for item in items.iter() {
                if let AstNode::Hash {
                    pool_index: h_p,
                    length: h_l,
                } = item
                {
                    let pairs = arena.get_hash_items(*h_p as usize, *h_l as usize);
                    assert_eq!(pairs.len(), 1);
                    assert_eq!(pairs[0].0, "letter");
                } else {
                    panic!("Expected hash in array, got {:?}", item);
                }
            }
        } else {
            panic!("Expected array, got {:?}", result);
        }
    }

    #[test]
    fn test_named_capture_before_repeat() {
        // Grammar: re("[a-z]+").repeat(1, None).label("word")
        let grammar = GrammarBuilder::new()
            .rule("word", re("[a-z]+").repeat(1, None).label("word"))
            .build();

        let (result, arena) = parse_and_transform("hello", &grammar);

        // Should produce: {:word => "hello"}
        if let AstNode::Hash { pool_index, length } = result {
            let pairs = arena.get_hash_items(pool_index as usize, length as usize);
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].0, "word");
        } else {
            panic!("Expected hash, got {:?}", result);
        }
    }

    #[test]
    fn test_same_outer_key_repetition() {
        // Grammar: A.repeat.label("x") >> B.repeat.label("x")
        // This produces: [{x: A1}, {x: A2}, {x: B1}, {x: B2}]
        // All hashes have the same outer key "x" -> should be REPETITION
        let grammar = GrammarBuilder::new()
            .rule(
                "test",
                seq(vec![
                    str("a").label("x").repeat(1, None),
                    str("b").label("x").repeat(1, None),
                ]),
            )
            .build();

        let (result, arena) = parse_and_transform("ab", &grammar);

        // Should produce an ARRAY (repetition pattern), not a merged hash
        match result {
            AstNode::Array { pool_index, length } => {
                let items = arena.get_array(pool_index as usize, length as usize);
                assert_eq!(items.len(), 2, "should have 2 items in array");
            }
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(pool_index as usize, length as usize);
                panic!(
                    "Expected array, got hash with {} keys: {:?}",
                    pairs.len(),
                    pairs.iter().map(|(k, _)| k).collect::<Vec<_>>()
                );
            }
            _ => panic!("Expected array, got {:?}", result),
        }
    }

    #[test]
    fn test_separator_repetition_pattern() {
        // Grammar: item >> (separator >> item).repeat  (X (',' X)*)
        // The first item and repetition items share the same key (:name).
        // The has_duplicate_labels check must NOT incorrectly merge these.
        // This is the pattern used in EXPRESS USE clauses:
        //   namedTypeOrRename >> (op_comma >> namedTypeOrRename).repeat
        let grammar = GrammarBuilder::new()
            .rule(
                "list",
                seq(vec![
                    dynamic(ref_("item")),
                    dynamic(seq(vec![dynamic(ref_("sep")), dynamic(ref_("item"))]).many()),
                ]),
            )
            .rule("item", re("[a-z]+").label("name"))
            .rule("sep", str(","))
            .build();

        let (result, arena) = parse_and_transform("a,b", &grammar);

        // The result should be an array with 2 items (not merged into 1)
        match result {
            AstNode::Array { pool_index, length } => {
                let items = arena.get_array(pool_index as usize, length as usize);
                assert_eq!(
                    items.len(),
                    2,
                    "should have 2 items in array, got {}: {:?}",
                    items.len(),
                    items
                );

                // Both items should be hashes with key "name"
                for (i, item) in items.iter().enumerate() {
                    if let AstNode::Hash {
                        pool_index: h_p,
                        length: h_l,
                    } = item
                    {
                        let pairs = arena.get_hash_items(*h_p as usize, *h_l as usize);
                        assert_eq!(
                            pairs.len(),
                            1,
                            "item {} should have 1 key, got {}: {:?}",
                            i,
                            pairs.len(),
                            pairs.iter().map(|(k, _)| k).collect::<Vec<_>>()
                        );
                        assert_eq!(pairs[0].0, "name", "item {} should have key 'name'", i);
                    } else {
                        panic!("Expected hash for item {}, got {:?}", i, item);
                    }
                }
            }
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(pool_index as usize, length as usize);
                panic!(
                    "Expected array, got hash with {} keys: {:?}",
                    pairs.len(),
                    pairs.iter().map(|(k, _)| k).collect::<Vec<_>>()
                );
            }
            _ => panic!("Expected array, got {:?}", result),
        }
    }

    #[test]
    fn test_literal_colon_not_stripped_as_tag() {
        // Grammar: str("--") >> match("[0-9IP:]").repeat(1, None), all
        // labeled "m". A ':' in the input is content, not a tag: the
        // joined string must keep every character. (Regressed as
        // "--IP1:" remarks truncating to "--I" when colons were
        // stripped anywhere in an array.)
        let grammar = GrammarBuilder::new()
            .rule(
                "test",
                seq(vec![
                    dynamic(str("--")),
                    dynamic(re("[0-9IP:]").repeat(1, None)),
                ])
                .label("m"),
            )
            .build();

        let (result, arena) = parse_and_transform("--IP1:", &grammar);

        let text = match &result {
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                assert_eq!(pairs.len(), 1);
                match &pairs[0].1 {
                    AstNode::StringRef { pool_index } => {
                        arena.get_string(*pool_index as usize).to_string()
                    }
                    AstNode::InputRef { offset, length } => "--IP1:"
                        .get(*offset as usize..(*offset + *length) as usize)
                        .expect("valid range")
                        .to_string(),
                    other => panic!("expected string, got {:?}", other),
                }
            }
            other => panic!("expected hash, got {:?}", other),
        };
        assert_eq!(text, "--IP1:");
    }

    #[test]
    fn test_named_separator_repetition_pattern() {
        // Same shape as test_separator_repetition_pattern but the
        // separator is a named capture — exactly the EXPRESS
        // `op_comma` / `op_delim` / `op_colon` pattern that the
        // previous flatten_sequence dropped when the inner .as made
        // sibling hashes multi-key. The fold-based path keeps the
        // Hash+Array hoist.
        let grammar = GrammarBuilder::new()
            .rule(
                "list",
                seq(vec![
                    dynamic(ref_("item")),
                    dynamic(seq(vec![dynamic(ref_("sep")), dynamic(ref_("item"))]).many()),
                ]),
            )
            .rule("item", re("[a-z]+").label("name"))
            .rule("sep", str(",").label("sep"))
            .build();

        let (result, arena) = parse_and_transform("a,b,c", &grammar);

        match result {
            AstNode::Array { pool_index, length } => {
                let items = arena.get_array(pool_index as usize, length as usize);
                assert_eq!(
                    items.len(),
                    3,
                    "should have 3 items in array, got {}: {:?}",
                    items.len(),
                    items
                );
                if let AstNode::Hash { pool_index: h_p, length: h_l } = &items[0] {
                    let pairs = arena.get_hash_items(*h_p as usize, *h_l as usize);
                    assert_eq!(pairs.len(), 1, "item 0 should have 1 key, got {:?}", pairs.iter().map(|(k,_)|k).collect::<Vec<_>>());
                    assert_eq!(pairs[0].0, "name");
                } else {
                    panic!("expected name hash for item 0, got {:?}", items[0]);
                }
                for (i, item) in items.iter().enumerate().skip(1) {
                    if let AstNode::Hash { pool_index: h_p, length: h_l } = item {
                        let pairs = arena.get_hash_items(*h_p as usize, *h_l as usize);
                        let keys: std::collections::HashSet<_> =
                            pairs.iter().map(|(k, _)| k.as_str()).collect();
                        assert!(
                            keys.contains("name") && keys.contains("sep"),
                            "item {i} should have sep+name keys, got {:?}",
                            pairs.iter().map(|(k, _)| k).collect::<Vec<_>>()
                        );
                    } else {
                        panic!("expected sep+name hash, got {:?}", item);
                    }
                }
            }
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(pool_index as usize, length as usize);
                panic!(
                    "Expected array, got hash with {} keys: {:?}",
                    pairs.len(),
                    pairs.iter().map(|(k, _)| k).collect::<Vec<_>>()
                );
            }
            other => panic!("Expected array, got {:?}", other),
        }
    }

    #[test]
    fn test_named_separator_multi_attribute() {
        // EXACT EXPRESS shape from grammar/parser.rb:314 — the multi-
        // parameter / multi-attribute bug. Each list item is either
        // {id} (first) or {comma, id} (rest), hoisted into an Array
        // by the fold — every id survives, not just the last.
        let grammar = GrammarBuilder::new()
            .rule(
                "list",
                seq(vec![
                    dynamic(ref_("id")),
                    dynamic(seq(vec![dynamic(ref_("comma")), dynamic(ref_("id"))]).many()),
                ]),
            )
            .rule("id", re("[a-z]+").label("id"))
            .rule("comma", str(",").label("comma"))
            .build();

        let (result, arena) = parse_and_transform("a,b,c,d", &grammar);

        match result {
            AstNode::Array { pool_index, length } => {
                let items = arena.get_array(pool_index as usize, length as usize);
                assert_eq!(items.len(), 4, "should have 4 items, got {}", items.len());
                for (i, item) in items.iter().enumerate() {
                    if let AstNode::Hash { pool_index: h_p, length: h_l } = item {
                        let pairs = arena.get_hash_items(*h_p as usize, *h_l as usize);
                        let keys: std::collections::HashSet<_> =
                            pairs.iter().map(|(k, _)| k.as_str()).collect();
                        assert!(keys.contains("id"), "item {i} missing id");
                        if i > 0 {
                            assert!(keys.contains("comma"), "item {i} missing comma");
                        }
                    } else {
                        panic!("expected id hash, got {:?}", item);
                    }
                }
            }
            other => panic!("Expected array, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_sequence_flattens_to_empty_string() {
        // Grammar: str("E") >> (str("A").optional >> str("B").optional).label("m") >> str("z")
        // A labeled sequence whose content matched nothing flattens to ""
        // (Ruby native semantics; e.g. EXPRESS entityHead.subsuper with
        // neither SUPERTYPE nor SUBTYPE present).
        let grammar = GrammarBuilder::new()
            .rule(
                "test",
                seq(vec![
                    dynamic(str("E")),
                    dynamic(seq(vec![str("A").optional(), str("B").optional()]).label("m")),
                    dynamic(str("z")),
                ]),
            )
            .build();

        let (result, arena) = parse_and_transform("Ez", &grammar);

        match result {
            AstNode::Hash { pool_index, length } => {
                let pairs = arena.get_hash_items(pool_index as usize, length as usize);
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0].0, "m");
                let text = match &pairs[0].1 {
                    AstNode::StringRef { pool_index } => {
                        arena.get_string(*pool_index as usize).to_string()
                    }
                    AstNode::InputRef { offset, length } => arena
                        .get_input()
                        .get(*offset as usize..(*offset + *length) as usize)
                        .expect("valid input range")
                        .to_string(),
                    other => panic!("expected empty string, got {:?}", other),
                };
                assert_eq!(text, "");
            }
            other => panic!("expected hash, got {:?}", other),
        }
    }
}
