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

            // Strip tags (strings starting with ':') from the items
            // Tags like ":sequence", ":repetition" are metadata, not content
            let mut tagged_items: Vec<AstNode> = Vec::with_capacity(items.len());
            for item in items.iter() {
                if !is_tag_node(item, arena) {
                    tagged_items.push(item.clone());
                }
            }

            let transformed_items: Vec<AstNode> = tagged_items
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

/// Transform a single-key hash (the common case)
///
/// Single-key hashes are produced by named captures like `.label("name")`.
fn transform_single_key_hash(
    pair: &(String, AstNode),
    arena: &mut AstArena,
    input: &str,
) -> AstNode {
    let (key, value) = pair;
    let key_str = key.as_str();
    let transformed = to_parslet_compatible(value, arena, input);

    // Check if this is a repetition result (array of named items)
    if let AstNode::Array { pool_index, length } = transformed {
        let items = arena.get_array(pool_index as usize, length as usize);

        // Check if all items are hashes with the same key
        if items.len() > 1
            && items
                .iter()
                .all(|item| is_hash_with_key(item, key_str, arena))
        {
            // This is a named repetition: return as array of hashes
            let (new_pool_idx, new_len) = arena.store_hash(&[(key_str, transformed)]);
            return AstNode::Hash {
                pool_index: new_pool_idx,
                length: new_len,
            };
        }
    }

    // Default: wrap transformed value with the key
    let (pool_idx, len) = arena.store_hash(&[(key_str, transformed)]);
    AstNode::Hash {
        pool_index: pool_idx,
        length: len,
    }
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
    eprintln!("flatten_sequence called with {} items", items.len());

    if items.is_empty() {
        return AstNode::Array {
            pool_index: 0,
            length: 0,
        };
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

    // Check if items have single keys or multiple keys
    // - Single key items with repeated outer key = true repetition (keep array)
    // - Multiple key items with repeated outer key = duplicate labels in sequence (merge)
    let max_keys_per_item = items
        .iter()
        .map(|item| match item {
            AstNode::Hash { pool_index, length } => *length as usize,
            _ => 0,
        })
        .max()
        .unwrap_or(0);

    // DUPLICATE LABELS IN SEQUENCE: multiple keys per item with repeated outer key
    // Example: [{group: {char: 'a'}}, {group: {digit: '5'}}]
    // Ruby semantics: merge with last value wins for the outer key
    // This is different from true repetition where each item has exactly one key
    let has_duplicate_labels = has_repetition && max_keys_per_item > 1;

    if has_repetition {
        if has_duplicate_labels {
            // DUPLICATE LABELS PATTERN: items have multiple keys with repeated outer key
            // This is a SEQUENCE with duplicate .as() labels
            // Ruby semantics: merge and keep last value for the outer key
            eprintln!("  -> DUPLICATE LABELS PATTERN, merging with last value wins");

            // Collect first item with its keys, then merge subsequent items
            if items.is_empty() {
                let (pool_idx, len) = arena.store_array(items);
                return AstNode::Array {
                    pool_index: pool_idx,
                    length: len,
                };
            }

            // Start with first item's key-value pairs
            let first_pairs: Vec<(String, AstNode)> = match &items[0] {
                AstNode::Hash { pool_index, length } => {
                    arena.get_hash_items(*pool_index as usize, *length as usize)
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                }
                _ => vec![],
            };

            // Track which key is the duplicate (appears in multiple items)
            let duplicate_key = key_counts
                .iter()
                .find(|(_, &count)| count > 1)
                .map(|(k, _)| k.clone());

            // Merge subsequent items, with last value winning for duplicate key
            let mut merged: Vec<(String, AstNode)> = first_pairs;
            for item in items.iter().skip(1) {
                if let AstNode::Hash { pool_index, length } = item {
                    let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                    for (k, v) in pairs {
                        if let Some(ref dup_key) = duplicate_key {
                            if k.as_str() == dup_key.as_str() {
                                // Replace the value for the duplicate key
                                if let Some(pos) = merged.iter().position(|(key, _)| key.as_str() == k.as_str()) {
                                    merged[pos] = (k.clone(), v.clone());
                                } else {
                                    merged.push((k.clone(), v.clone()));
                                }
                                continue;
                            }
                        }
                        // Add non-duplicate keys
                        if !merged.iter().any(|(key, _)| key.as_str() == k.as_str()) {
                            merged.push((k.clone(), v.clone()));
                        }
                    }
                }
            }

            // If we have a duplicate key, wrap in a hash; otherwise return array
            if let Some(ref dup_key) = duplicate_key {
                // Get the last value for the duplicate key and wrap it
                if let Some((_, last_value)) = merged.iter().find(|(k, _)| k == dup_key) {
                    let (pool_idx, len) = arena.store_hash(&[(dup_key.as_str(), last_value.clone())]);
                    return AstNode::Hash {
                        pool_index: pool_idx,
                        length: len,
                    };
                }
            }

            // Fallback: return array
            let (pool_idx, len) = arena.store_array(items);
            return AstNode::Array {
                pool_index: pool_idx,
                length: len,
            };
        } else {
            // TRUE REPETITION: each item has exactly one key
            // Keep as array of hashes
            // Example: [{letter: 'a'}, {letter: 'b'}] or [{schemaDecl: ...}, {schemaDecl: ...}]
            eprintln!("  -> TRUE REPETITION PATTERN, keeping array");
            let (pool_idx, len) = arena.store_array(items);
            return AstNode::Array {
                pool_index: pool_idx,
                length: len,
            };
        }
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

            eprintln!(
                "  hash_count={}, total_items={}, first_key={}, all_same_key={}",
                hash_count, total_items, first_key, all_same_key
            );

            if all_same_key {
                // ALL hashes have the SAME outer key -> REPETITION pattern
                // Keep items as array (do NOT merge)
                // This matches Ruby's flatten_sequence: "return items unless all_values_are_hashes"
                eprintln!("  -> ALL SAME KEY, returning array");
                let (pool_idx, len) = arena.store_array(items);
                return AstNode::Array {
                    pool_index: pool_idx,
                    length: len,
                };
            } else {
                // DIFFERENT outer keys -> WRAPPER pattern
                // Merge all inner hashes into a single hash under a synthetic key
                eprintln!("  -> DIFFERENT KEYS, WRAPPER PATTERN");
                let mut merged_inner: Vec<(String, AstNode)> = Vec::new();
                for item in items {
                    if let AstNode::Hash { pool_index, length } = item {
                        let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                        for (k, v) in pairs {
                            merged_inner.push((k.clone(), v));
                        }
                    }
                }
                // Convert to borrowed slices for store_hash
                let inner_refs: Vec<(&str, AstNode)> = merged_inner
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.clone()))
                    .collect();
                let (inner_pool, inner_len) = arena.store_hash(&inner_refs);
                // Use first key as wrapper key (any key works since we're merging)
                let (pool_idx, len) = arena.store_hash(&[(
                    first_key.as_str(),
                    AstNode::Hash {
                        pool_index: inner_pool,
                        length: inner_len,
                    },
                )]);
                return AstNode::Hash {
                    pool_index: pool_idx,
                    length: len,
                };
            }
        }

        // Mixed keys or multiple keys: keep as array
        eprintln!("  -> NO SINGLE KEY, returning array");
        let (pool_idx, len) = arena.store_array(items);
        return AstNode::Array {
            pool_index: pool_idx,
            length: len,
        };
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
    use crate::portable::parser_dsl::{dynamic, re, seq, str, GrammarBuilder, ParsletExt};

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
}
