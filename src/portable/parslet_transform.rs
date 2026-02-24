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
            let transformed_items: Vec<AstNode> = items
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
        other => *other,
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
        .map(|(k, v)| (k.as_str(), *v))
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

    if items.len() == 1 {
        return items[0];
    }

    // First pass: collect all data without mutating arena
    // Use owned Strings for keys to avoid lifetime issues
    let mut merged_hash: Vec<(String, AstNode)> = Vec::new();
    let mut string_parts: Vec<String> = Vec::new();
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
                // Get string from input
                let start = *offset as usize;
                let end = start + *length as usize;
                if let Some(s) = input.get(start..end) {
                    string_parts.push(s.to_string());
                }
                total_items += 1;
            }
            AstNode::StringRef { pool_index } => {
                let (s, _, _) = arena.get_string_parts(*pool_index as usize);
                string_parts.push(s.to_string());
                total_items += 1;
            }
            AstNode::Array { pool_index, length } => {
                // Flatten nested arrays
                let nested = arena.get_array(*pool_index as usize, *length as usize);
                for nested_item in nested {
                    match nested_item {
                        AstNode::Hash {
                            pool_index: p,
                            length: l,
                        } => {
                            let pairs = arena.get_hash_items(p as usize, l as usize);
                            for (k, v) in pairs {
                                if let Some(pos) = merged_hash.iter().position(|(key, _)| *key == k)
                                {
                                    merged_hash[pos] = (k.clone(), v);
                                } else {
                                    merged_hash.push((k.clone(), v));
                                }
                            }
                            hash_count += 1;
                        }
                        AstNode::InputRef { offset, length } => {
                            let start = offset as usize;
                            let end = start + length as usize;
                            if let Some(s) = input.get(start..end) {
                                string_parts.push(s.to_string());
                            }
                        }
                        AstNode::StringRef { pool_index } => {
                            let (s, _, _) = arena.get_string_parts(pool_index as usize);
                            string_parts.push(s.to_string());
                        }
                        _ => {}
                    }
                }
                total_items += 1;
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
            let first_key_clone = first_key.clone();
            let all_same_key = items
                .iter()
                .all(|item| get_single_key(item, arena).is_some_and(|k| k == first_key_clone));

            if all_same_key {
                // Check if values are hashes (wrapper) or simple (repetition)
                let all_values_are_hashes = items.iter().all(|item| {
                    if let AstNode::Hash { pool_index, length } = item {
                        let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                        if pairs.len() == 1 {
                            matches!(pairs[0].1, AstNode::Hash { .. })
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });

                if all_values_are_hashes {
                    // Wrapper pattern: merge inner hashes
                    let mut merged_inner: Vec<(String, AstNode)> = Vec::new();
                    for item in items {
                        if let AstNode::Hash { pool_index, length } = item {
                            let pairs =
                                arena.get_hash_items(*pool_index as usize, *length as usize);
                            if pairs.len() == 1 {
                                if let AstNode::Hash {
                                    pool_index: inner_p,
                                    length: inner_l,
                                } = pairs[0].1
                                {
                                    let inner_pairs =
                                        arena.get_hash_items(inner_p as usize, inner_l as usize);
                                    for (k, v) in inner_pairs {
                                        if let Some(pos) =
                                            merged_inner.iter().position(|(key, _)| *key == k)
                                        {
                                            merged_inner[pos] = (k.clone(), v);
                                        } else {
                                            merged_inner.push((k.clone(), v));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Convert to borrowed slices for store_hash
                    let inner_refs: Vec<(&str, AstNode)> =
                        merged_inner.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                    let (inner_pool, inner_len) = arena.store_hash(&inner_refs);
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
                } else {
                    // Repetition pattern: keep as array
                    let (pool_idx, len) = arena.store_array(items);
                    return AstNode::Array {
                        pool_index: pool_idx,
                        length: len,
                    };
                }
            }
        }

        // Mixed keys or multiple keys: keep as array
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
        let hash_refs: Vec<(&str, AstNode)> =
            merged_hash.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let (pool_idx, len) = arena.store_hash(&hash_refs);
        return AstNode::Hash {
            pool_index: pool_idx,
            length: len,
        };
    }

    // No named captures - handle strings
    if !string_parts.is_empty() {
        if string_parts.len() == 1 {
            // Return single string as StringRef
            return arena.intern_string(&string_parts[0]);
        } else {
            // Join strings
            let joined: String = string_parts.join("");
            return arena.intern_string(&joined);
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
        items[0]
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
        let grammar = GrammarBuilder::new()
            .rule("letters", re("[a-z]").label("letter").repeat(1, None))
            .build();

        let (result, arena) = parse_and_transform("abc", &grammar);

        // For repetition pattern with named captures BEFORE repeat,
        // the result should be an ARRAY of hashes, not a single hash
        if let AstNode::Array { pool_index, length } = result {
            let items = arena.get_array(pool_index as usize, length as usize);
            assert_eq!(items.len(), 3);

            // Each item should be a hash with key "letter"
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
}
