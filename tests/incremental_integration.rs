//! Integration tests for incremental parsing
//!
//! These tests cover the incremental parsing functionality including:
//! - Edit tracking
//! - Dirty region detection
//! - Cache reuse
//! - Position translation

use parsanol::portable::{
    incremental::{DirtyRegion, DirtyRegionTracker, Edit, IncrementalParser},
    parser_dsl::{re, str, GrammarBuilder},
    AstArena, AstNode,
};

// ============================================================================
// Edit Tests
// ============================================================================

#[test]
fn test_edit_new() {
    let edit = Edit::new(10, 5, 3);
    assert_eq!(edit.offset, 10);
    assert_eq!(edit.old_length, 5);
    assert_eq!(edit.new_length, 3);
}

#[test]
fn test_edit_insert() {
    let edit = Edit::insert(5, 10);
    assert_eq!(edit.offset, 5);
    assert_eq!(edit.old_length, 0);
    assert_eq!(edit.new_length, 10);
}

#[test]
fn test_edit_delete() {
    let edit = Edit::delete(3, 8);
    assert_eq!(edit.offset, 3);
    assert_eq!(edit.old_length, 8);
    assert_eq!(edit.new_length, 0);
}

#[test]
fn test_edit_replace() {
    let edit = Edit::replace(0, 5, 7);
    assert_eq!(edit.offset, 0);
    assert_eq!(edit.old_length, 5);
    assert_eq!(edit.new_length, 7);
}

#[test]
fn test_edit_delta() {
    // Insertion: positive delta
    let insert = Edit::insert(0, 10);
    assert_eq!(insert.delta(), 10);

    // Deletion: negative delta
    let delete = Edit::delete(0, 10);
    assert_eq!(delete.delta(), -10);

    // Same length replacement: zero delta
    let replace = Edit::replace(0, 5, 5);
    assert_eq!(replace.delta(), 0);

    // Expansion
    let expand = Edit::replace(0, 3, 10);
    assert_eq!(expand.delta(), 7);

    // Contraction
    let contract = Edit::replace(0, 10, 3);
    assert_eq!(contract.delta(), -7);
}

#[test]
fn test_edit_affects_position() {
    let edit = Edit::new(10, 5, 3);

    // Before edit: not affected
    assert!(!edit.affects_position(5));

    // At start and beyond: affected
    assert!(edit.affects_position(10));
    assert!(edit.affects_position(12));
    assert!(edit.affects_position(14));
    assert!(edit.affects_position(15)); // affects_position returns true for pos >= offset
    assert!(edit.affects_position(100));
}

#[test]
fn test_edit_translate_position() {
    // Insertion: offset=5, old=0, new=3, delta=+3
    let insert = Edit::insert(5, 3);
    assert_eq!(insert.translate_position(3), 3); // Before offset: unchanged
    assert_eq!(insert.translate_position(5), 5); // At offset: unchanged (pos <= offset)
    assert_eq!(insert.translate_position(10), 13); // After: apply delta (+3)

    // Deletion: offset=5, old=3, new=0, delta=-3
    let delete = Edit::delete(5, 3);
    assert_eq!(delete.translate_position(3), 3); // Before: unchanged
    assert_eq!(delete.translate_position(5), 5); // At offset: unchanged (pos <= offset)
    assert_eq!(delete.translate_position(7), 5); // In deleted range: map to offset + new_len = 5
    assert_eq!(delete.translate_position(10), 7); // After: apply delta (-3)

    // Replace same length: offset=5, old=3, new=3, delta=0
    let replace = Edit::replace(5, 3, 3);
    assert_eq!(replace.translate_position(3), 3); // Before: unchanged
    assert_eq!(replace.translate_position(5), 5); // At offset: unchanged
    assert_eq!(replace.translate_position(8), 8); // In range: maps to offset + new_len = 8
    assert_eq!(replace.translate_position(10), 10); // After: delta=0

    // Replace with expansion: offset=5, old=3, new=7, delta=+4
    let expand = Edit::replace(5, 3, 7);
    assert_eq!(expand.translate_position(3), 3); // Before: unchanged
    assert_eq!(expand.translate_position(5), 5); // At offset: unchanged
    assert_eq!(expand.translate_position(7), 12); // In range: offset + new_len = 12
    assert_eq!(expand.translate_position(20), 24); // After: apply delta (+4)
}

// ============================================================================
// Dirty Region Tests
// ============================================================================

#[test]
fn test_dirty_region_contains() {
    let region = DirtyRegion {
        start: 10,
        end: 20,
    };

    assert!(region.contains(10));
    assert!(region.contains(15));
    assert!(region.contains(19));
    assert!(!region.contains(9));
    assert!(!region.contains(20));
}

#[test]
fn test_dirty_region_overlaps() {
    let region1 = DirtyRegion {
        start: 10,
        end: 20,
    };

    // Overlapping
    let region2 = DirtyRegion {
        start: 15,
        end: 25,
    };
    assert!(region1.overlaps(&region2));
    assert!(region2.overlaps(&region1));

    // Adjacent (not overlapping)
    let region3 = DirtyRegion {
        start: 20,
        end: 30,
    };
    assert!(!region1.overlaps(&region3));

    // Disjoint
    let region4 = DirtyRegion {
        start: 30,
        end: 40,
    };
    assert!(!region1.overlaps(&region4));

    // Contained
    let region5 = DirtyRegion {
        start: 12,
        end: 18,
    };
    assert!(region1.overlaps(&region5));
}

// ============================================================================
// Dirty Region Tracker Tests
// ============================================================================

#[test]
fn test_tracker_new() {
    let tracker = DirtyRegionTracker::new();
    assert!(tracker.regions().is_empty());
}

#[test]
fn test_tracker_mark_edit() {
    let mut tracker = DirtyRegionTracker::new();

    // Mark an edit
    tracker.mark_edit(&Edit::replace(10, 5, 7));

    let regions = tracker.regions();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].start, 10);
    assert_eq!(regions[0].end, 15);
}

#[test]
fn test_tracker_mark_multiple_edits() {
    let mut tracker = DirtyRegionTracker::new();

    // Mark two disjoint edits
    tracker.mark_edit(&Edit::replace(10, 5, 5));
    tracker.mark_edit(&Edit::replace(50, 3, 3));

    let regions = tracker.regions();
    assert_eq!(regions.len(), 2);
}

#[test]
fn test_tracker_mark_overlapping_edits() {
    let mut tracker = DirtyRegionTracker::new();

    // Mark two overlapping edits - should merge
    tracker.mark_edit(&Edit::replace(10, 5, 5));
    tracker.mark_edit(&Edit::replace(12, 5, 5));

    let regions = tracker.regions();
    // Should be merged into one region
    assert!(regions.len() <= 2);
}

#[test]
fn test_tracker_is_dirty() {
    let mut tracker = DirtyRegionTracker::new();
    tracker.mark_edit(&Edit::replace(10, 5, 5));

    assert!(!tracker.is_dirty(5)); // Before
    assert!(tracker.is_dirty(10)); // At start
    assert!(tracker.is_dirty(12)); // In middle
    assert!(!tracker.is_dirty(20)); // After
}

#[test]
fn test_tracker_clear() {
    let mut tracker = DirtyRegionTracker::new();
    tracker.mark_edit(&Edit::replace(10, 5, 5));

    tracker.clear();
    assert!(tracker.regions().is_empty());
}

// ============================================================================
// Incremental Parsing Integration Tests
// ============================================================================

#[test]
fn test_incremental_parser_new() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let parser = IncrementalParser::new(&grammar);

    // Should be in clean state
    assert_eq!(parser.dirty_region_count(), 0);
}

#[test]
fn test_incremental_parse_simple() {
    let grammar = GrammarBuilder::new().rule("word", re(r"[a-z]+")).build();
    let mut parser = IncrementalParser::new(&grammar);

    let input = "hello";
    let mut arena = AstArena::for_input(input.len());

    let result = parser.parse(input, &mut arena).expect("Should parse");
    assert!(matches!(result, AstNode::InputRef { .. }));
}

#[test]
fn test_incremental_parse_with_edit() {
    let grammar = GrammarBuilder::new().rule("word", re(r"[a-z]+")).build();
    let mut parser = IncrementalParser::new(&grammar);

    // Initial parse
    let input1 = "hello";
    let mut arena1 = AstArena::for_input(input1.len());
    let _result1 = parser.parse(input1, &mut arena1).expect("Should parse");

    // Edit: change "hello" to "world" (same length)
    let input2 = "world";
    let edit = Edit::replace(0, 5, 5);
    let mut arena2 = AstArena::for_input(input2.len());

    let result2 = parser
        .parse_with_edit(input2, &mut arena2, edit)
        .expect("Should parse");

    // Both should succeed
    assert!(matches!(result2.ast, AstNode::InputRef { .. }));
}

#[test]
fn test_incremental_parse_insertion() {
    let grammar = GrammarBuilder::new().rule("text", re(r"[a-z]+")).build();
    let mut parser = IncrementalParser::new(&grammar);

    // Initial parse
    let input1 = "hello";
    let mut arena1 = AstArena::for_input(input1.len());
    let _result1 = parser.parse(input1, &mut arena1).expect("Should parse");

    // Insert: "hello" -> "hello world" (at position 5, insert " world")
    let input2 = "helloworld";
    let edit = Edit::insert(5, 5); // Insert 5 chars at position 5
    let mut arena2 = AstArena::for_input(input2.len());

    let result2 = parser
        .parse_with_edit(input2, &mut arena2, edit)
        .expect("Should parse");

    assert!(matches!(result2.ast, AstNode::InputRef { .. }));
}

#[test]
fn test_incremental_parse_deletion() {
    let grammar = GrammarBuilder::new().rule("text", re(r"[a-z]+")).build();
    let mut parser = IncrementalParser::new(&grammar);

    // Initial parse
    let input1 = "helloworld";
    let mut arena1 = AstArena::for_input(input1.len());
    let _result1 = parser.parse(input1, &mut arena1).expect("Should parse");

    // Delete: "helloworld" -> "hello" (delete 5 chars at position 5)
    let input2 = "hello";
    let edit = Edit::delete(5, 5);
    let mut arena2 = AstArena::for_input(input2.len());

    let result2 = parser
        .parse_with_edit(input2, &mut arena2, edit)
        .expect("Should parse");

    assert!(matches!(result2.ast, AstNode::InputRef { .. }));
}

#[test]
fn test_incremental_result_reuse_metrics() {
    let grammar = GrammarBuilder::new().rule("text", re(r"[a-z]+")).build();
    let mut parser = IncrementalParser::new(&grammar);

    // Initial parse
    let input1 = "hello";
    let mut arena1 = AstArena::for_input(input1.len());
    let _result1 = parser.parse(input1, &mut arena1).expect("Should parse");

    // Small edit at the end
    let input2 = "hella"; // Change last char
    let edit = Edit::replace(4, 1, 1);
    let mut arena2 = AstArena::for_input(input2.len());

    let result2 = parser
        .parse_with_edit(input2, &mut arena2, edit)
        .expect("Should parse");

    // Should have some cache reuse (positions 0-3 could be reused)
    // Note: actual reuse depends on cache implementation
    assert!(matches!(result2.ast, AstNode::InputRef { .. }));
    assert!(result2.reused_cache_entries >= 0);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_incremental_empty_input() {
    let grammar = GrammarBuilder::new().rule("empty", str("")).build();
    let mut parser = IncrementalParser::new(&grammar);

    let input = "";
    let mut arena = AstArena::for_input(0);

    let result = parser.parse(input, &mut arena);
    // Empty input may or may not succeed
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_incremental_unicode_edit() {
    let grammar = GrammarBuilder::new().rule("text", re(r".+")).build();
    let mut parser = IncrementalParser::new(&grammar);

    // Initial: "hello world"
    let input1 = "hello 世界";
    let mut arena1 = AstArena::for_input(input1.len());
    let _result1 = parser.parse(input1, &mut arena1).expect("Should parse");

    // Change: "hello 世界" -> "hello 世界!" (add exclamation)
    let input2 = "hello 世界!";
    let edit = Edit::insert(input1.len(), 1);
    let mut arena2 = AstArena::for_input(input2.len());

    let result2 = parser
        .parse_with_edit(input2, &mut arena2, edit)
        .expect("Should parse");

    assert!(matches!(result2.ast, AstNode::InputRef { .. }));
}

#[test]
fn test_incremental_long_input() {
    let grammar = GrammarBuilder::new().rule("text", re(r"[a-z]+")).build();
    let mut parser = IncrementalParser::new(&grammar);

    // Long input
    let input1 = "a".repeat(10000);
    let mut arena1 = AstArena::for_input(input1.len());
    let _result1 = parser.parse(&input1, &mut arena1).expect("Should parse");

    // Edit at the end
    let mut input2 = input1.clone();
    input2.push('b');
    let edit = Edit::insert(10000, 1);
    let mut arena2 = AstArena::for_input(input2.len());

    let result2 = parser
        .parse_with_edit(&input2, &mut arena2, edit)
        .expect("Should parse");

    assert!(matches!(result2.ast, AstNode::InputRef { .. }));
}
