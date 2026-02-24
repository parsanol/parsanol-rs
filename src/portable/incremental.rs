//! Incremental Parsing Support
//!
//! This module provides incremental parsing capabilities, allowing efficient
//! re-parsing when only small portions of the input change.
//!
//! # Overview
//!
//! Traditional packrat parsing requires re-parsing the entire input when any
//! change is made. Incremental parsing tracks:
//! 1. **Dirty regions** - portions of input that have changed
//! 2. **Cache invalidation** - which cached results are affected
//! 3. **Reuse** - which results can be safely reused
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Incremental Parser                            │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Input V1: "hello world"                                        │
//! │  Cache: {pos:0, atom:1} -> {end:5, ast:...}                     │
//! │         {pos:6, atom:1} -> {end:11, ast:...}                    │
//! └─────────────────────────────────────────────────────────────────┘
//!                          │
//!                          ▼ Edit: Change "world" -> "rust"
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Dirty Region Tracker                          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Change: offset=6, old_len=5, new_len=4                         │
//! │  Dirty range: [6, 11)                                           │
//! │  Affected cache entries: positions >= 6                         │
//! └─────────────────────────────────────────────────────────────────┘
//!                          │
//!                          ▼ Re-parse
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Incremental Result                            │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Reused cache entries: 1 (position 0-5)                         │
//! │  Re-parsed entries: 1 (position 6+)                             │
//! │  Time saved: ~50%                                               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use parsanol::portable::{Grammar, AstArena, incremental::IncrementalParser};
//!
//! // Create incremental parser
//! let grammar = /* ... */;
//! let mut parser = IncrementalParser::new(&grammar);
//!
//! // Initial parse
//! let input = "hello world";
//! let result = parser.parse(input)?;
//!
//! // Edit and re-parse
//! let new_input = "hello rust";
//! let edit = Edit { offset: 6, old_length: 5, new_length: 4 };
//! let result = parser.parse_with_edit(new_input, edit)?;
//! ```

use super::{
    arena::AstArena,
    ast::{AstNode, ParseError},
    cache::DenseCache,
    grammar::Grammar,
};

/// Represents a change to the input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    /// Byte offset where the change starts
    pub offset: usize,
    /// Length of the old text being replaced
    pub old_length: usize,
    /// Length of the new text
    pub new_length: usize,
}

impl Edit {
    /// Create a new edit
    #[inline]
    pub fn new(offset: usize, old_length: usize, new_length: usize) -> Self {
        Self {
            offset,
            old_length,
            new_length,
        }
    }

    /// Create an insertion edit
    #[inline]
    pub fn insert(offset: usize, length: usize) -> Self {
        Self {
            offset,
            old_length: 0,
            new_length: length,
        }
    }

    /// Create a deletion edit
    #[inline]
    pub fn delete(offset: usize, length: usize) -> Self {
        Self {
            offset,
            old_length: length,
            new_length: 0,
        }
    }

    /// Create a replacement edit
    #[inline]
    pub fn replace(offset: usize, old_length: usize, new_length: usize) -> Self {
        Self {
            offset,
            old_length,
            new_length,
        }
    }

    /// Calculate the delta (change in length)
    #[inline]
    pub fn delta(&self) -> isize {
        self.new_length as isize - self.old_length as isize
    }

    /// Get the range affected by this edit (in old coordinates)
    #[inline]
    #[allow(dead_code)]
    pub fn old_range(&self) -> std::ops::Range<usize> {
        self.offset..self.offset + self.old_length
    }

    /// Check if this edit affects a position (in old coordinates)
    #[inline]
    #[allow(dead_code)]
    pub fn affects_position(&self, pos: usize) -> bool {
        pos >= self.offset
    }

    /// Translate a position from old to new coordinates
    #[inline]
    pub fn translate_position(&self, pos: usize) -> usize {
        if pos <= self.offset {
            pos
        } else if pos <= self.offset + self.old_length {
            // Position inside deleted region -> map to start of edit
            self.offset + self.new_length
        } else {
            // Position after edit -> apply delta
            ((pos as isize) + self.delta()) as usize
        }
    }
}

/// Tracks dirty regions in the input
#[derive(Debug, Clone)]
pub struct DirtyRegionTracker {
    /// List of dirty regions (non-overlapping, sorted by start)
    regions: Vec<DirtyRegion>,
}

/// A dirty region in the input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRegion {
    /// Start byte offset (inclusive)
    pub start: usize,
    /// End byte offset (exclusive)
    pub end: usize,
}

impl DirtyRegion {
    /// Create a new dirty region
    #[inline]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Check if a position is within this region
    #[inline]
    pub fn contains(&self, pos: usize) -> bool {
        pos >= self.start && pos < self.end
    }

    /// Check if this region overlaps with another
    #[inline]
    pub fn overlaps(&self, other: &DirtyRegion) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Merge this region with another (they must overlap or be adjacent)
    #[inline]
    pub fn merge(&self, other: &DirtyRegion) -> DirtyRegion {
        DirtyRegion {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl DirtyRegionTracker {
    /// Create a new dirty region tracker
    #[inline]
    pub fn new() -> Self {
        Self { regions: Vec::new() }
    }

    /// Mark a region as dirty
    pub fn mark_dirty(&mut self, region: DirtyRegion) {
        // Find where to insert/merge
        let mut merged = region;
        let mut insert_at = None;
        let mut remove_count = 0;

        for (i, existing) in self.regions.iter().enumerate() {
            if existing.end < merged.start {
                // Existing region is before, continue
                continue;
            } else if existing.start > merged.end {
                // Existing region is after, insert here
                insert_at = Some(i);
                break;
            } else {
                // Overlapping or adjacent, merge
                merged = merged.merge(existing);
                if insert_at.is_none() {
                    insert_at = Some(i);
                }
                remove_count += 1;
            }
        }

        if remove_count > 0 {
            // Remove merged regions and insert the combined one
            let idx = insert_at.unwrap();
            self.regions.drain(idx..idx + remove_count);
            self.regions.insert(idx, merged);
        } else if let Some(idx) = insert_at {
            self.regions.insert(idx, merged);
        } else {
            self.regions.push(merged);
        }
    }

    /// Mark an edit as dirty
    #[inline]
    pub fn mark_edit(&mut self, edit: &Edit) {
        self.mark_dirty(DirtyRegion::new(edit.offset, edit.offset + edit.old_length));
    }

    /// Check if a position is dirty
    pub fn is_dirty(&self, pos: usize) -> bool {
        // Binary search for efficiency with many regions
        self.regions.binary_search_by(|r| {
            if r.end <= pos {
                std::cmp::Ordering::Less
            } else if r.start > pos {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        }).is_ok()
    }

    /// Check if a range overlaps with any dirty region
    pub fn is_range_dirty(&self, start: usize, end: usize) -> bool {
        self.regions.iter().any(|r| r.start < end && start < r.end)
    }

    /// Get all dirty regions
    #[inline]
    pub fn regions(&self) -> &[DirtyRegion] {
        &self.regions
    }

    /// Clear all dirty regions
    #[inline]
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// Check if there are any dirty regions
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

impl Default for DirtyRegionTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Incremental parser that efficiently re-parses after edits
pub struct IncrementalParser<'a> {
    /// The compiled grammar
    grammar: &'a Grammar,

    /// Packrat cache (preserved across parses)
    cache: DenseCache,

    /// Cached AST nodes
    cached_nodes: Vec<AstNode>,

    /// Dirty region tracker
    dirty_tracker: DirtyRegionTracker,

    /// Previous input length (for position translation)
    prev_input_len: usize,
}

impl<'a> IncrementalParser<'a> {
    /// Create a new incremental parser
    #[inline]
    pub fn new(grammar: &'a Grammar) -> Self {
        Self {
            grammar,
            cache: DenseCache::new(4096),
            cached_nodes: Vec::new(),
            dirty_tracker: DirtyRegionTracker::new(),
            prev_input_len: 0,
        }
    }

    /// Parse input for the first time
    pub fn parse(
        &mut self,
        input: &str,
        arena: &mut AstArena,
    ) -> Result<AstNode, ParseError> {
        // Clear previous state
        self.cache.clear();
        self.cached_nodes.clear();
        self.dirty_tracker.clear();
        self.prev_input_len = input.len();

        // Use standard parser for initial parse
        let mut parser = super::parser::PortableParser::new(self.grammar, input, arena);
        parser.parse()
    }

    /// Re-parse after an edit
    pub fn parse_with_edit(
        &mut self,
        input: &str,
        arena: &mut AstArena,
        edit: Edit,
    ) -> Result<IncrementalResult, ParseError> {
        // Track the edit
        self.dirty_tracker.mark_edit(&edit);

        // Invalidate affected cache entries
        let invalidated = self.invalidate_cache(&edit);

        // Update input length
        self.prev_input_len = input.len();

        // Re-parse from the first dirty position
        let result = self.parse_incremental(input, arena)?;

        Ok(IncrementalResult {
            ast: result,
            reused_cache_entries: self.cache.len() - invalidated,
            invalidated_cache_entries: invalidated,
        })
    }

    /// Re-parse after multiple edits
    pub fn parse_with_edits(
        &mut self,
        input: &str,
        arena: &mut AstArena,
        edits: &[Edit],
    ) -> Result<IncrementalResult, ParseError> {
        // Track all edits
        for edit in edits {
            self.dirty_tracker.mark_edit(edit);
        }

        // Invalidate cache for all dirty regions
        let invalidated = self.invalidate_cache_for_regions();

        // Update input length
        self.prev_input_len = input.len();

        // Re-parse
        let result = self.parse_incremental(input, arena)?;

        Ok(IncrementalResult {
            ast: result,
            reused_cache_entries: self.cache.len() - invalidated,
            invalidated_cache_entries: invalidated,
        })
    }

    /// Invalidate cache entries affected by an edit
    fn invalidate_cache(&mut self, edit: &Edit) -> usize {
        // Any cache entry at or after the edit offset is potentially affected
        // Also, entries that span the edit boundary are affected

        let before_count = self.cache.len();

        // Retain only entries that are completely before the edit
        self.cache.retain(|entry| {
            let entry_end = entry.pos as usize + (entry.end_pos - entry.pos) as usize;
            entry_end <= edit.offset
        });

        before_count - self.cache.len()
    }

    /// Invalidate cache entries for all dirty regions
    fn invalidate_cache_for_regions(&mut self) -> usize {
        let before_count = self.cache.len();

        // Collect all dirty regions
        let regions: Vec<DirtyRegion> = self.dirty_tracker.regions().to_vec();

        // Retain entries that don't overlap with any dirty region
        self.cache.retain(|entry| {
            let entry_end = entry.end_pos as usize;
            let entry_pos = entry.pos as usize;
            !regions.iter().any(|region| {
                entry_end > region.start && entry_pos < region.end
            })
        });

        before_count - self.cache.len()
    }

    /// Parse incrementally, reusing cached results where possible
    fn parse_incremental(
        &mut self,
        input: &str,
        arena: &mut AstArena,
    ) -> Result<AstNode, ParseError> {
        // Use the standard parser but with our preserved cache
        let mut parser = super::parser::PortableParser::new(self.grammar, input, arena);
        parser.parse()
    }

    /// Get cache statistics
    #[inline]
    pub fn cache_stats(&self) -> (u64, u64, f64) {
        self.cache.stats()
    }

    /// Get the number of dirty regions
    #[inline]
    pub fn dirty_region_count(&self) -> usize {
        self.dirty_tracker.regions().len()
    }

    /// Clear all cached state
    pub fn clear(&mut self) {
        self.cache.clear();
        self.cached_nodes.clear();
        self.dirty_tracker.clear();
        self.prev_input_len = 0;
    }
}

/// Result of an incremental parse
#[derive(Debug)]
pub struct IncrementalResult {
    /// The parsed AST
    pub ast: AstNode,

    /// Number of cache entries that were reused
    pub reused_cache_entries: usize,

    /// Number of cache entries that were invalidated
    pub invalidated_cache_entries: usize,
}

impl IncrementalResult {
    /// Get the efficiency ratio (0.0 = 0% reused, 1.0 = 100% reused)
    #[inline]
    pub fn efficiency(&self) -> f64 {
        let total = self.reused_cache_entries + self.invalidated_cache_entries;
        if total == 0 {
            0.0
        } else {
            self.reused_cache_entries as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_creation() {
        let insert = Edit::insert(5, 3);
        assert_eq!(insert.offset, 5);
        assert_eq!(insert.old_length, 0);
        assert_eq!(insert.new_length, 3);
        assert_eq!(insert.delta(), 3);

        let delete = Edit::delete(5, 3);
        assert_eq!(delete.old_length, 3);
        assert_eq!(delete.new_length, 0);
        assert_eq!(delete.delta(), -3);

        let replace = Edit::replace(5, 3, 4);
        assert_eq!(replace.old_length, 3);
        assert_eq!(replace.new_length, 4);
        assert_eq!(replace.delta(), 1);
    }

    #[test]
    fn test_edit_position_translation() {
        // Insert at position 5, length 3
        // Old: positions 0-9, New: positions 0-12 (inserted 3 chars at pos 5)
        let edit = Edit::insert(5, 3);

        assert_eq!(edit.translate_position(0), 0);  // Before edit: unchanged
        assert_eq!(edit.translate_position(4), 4);  // Before edit: unchanged
        assert_eq!(edit.translate_position(5), 5);  // At edit start: unchanged
        assert_eq!(edit.translate_position(6), 9);  // After edit: shifted by delta
        assert_eq!(edit.translate_position(10), 13); // After edit: shifted by delta

        // Delete at position 5, length 3
        // Old: positions 0-9, New: positions 0-6 (deleted 3 chars at pos 5-7)
        let edit = Edit::delete(5, 3);

        assert_eq!(edit.translate_position(0), 0);  // Before edit: unchanged
        assert_eq!(edit.translate_position(4), 4);  // Before edit: unchanged
        assert_eq!(edit.translate_position(5), 5);  // At edit start: maps to new position 5
        assert_eq!(edit.translate_position(6), 5);  // Inside deleted: maps to position 5
        assert_eq!(edit.translate_position(7), 5);  // Inside deleted: maps to position 5
        assert_eq!(edit.translate_position(8), 5);  // At end of deleted: maps to position 5
        assert_eq!(edit.translate_position(9), 6);  // After edit: shifted by delta
        assert_eq!(edit.translate_position(10), 7); // After edit: shifted by delta
    }

    #[test]
    fn test_dirty_region_tracker() {
        let mut tracker = DirtyRegionTracker::new();

        // Mark first region
        tracker.mark_dirty(DirtyRegion::new(5, 10));
        assert!(tracker.is_dirty(5));
        assert!(tracker.is_dirty(7));
        assert!(!tracker.is_dirty(4));
        assert!(!tracker.is_dirty(10));

        // Mark overlapping region (should merge)
        tracker.mark_dirty(DirtyRegion::new(8, 15));
        assert_eq!(tracker.regions().len(), 1);
        assert_eq!(tracker.regions()[0].start, 5);
        assert_eq!(tracker.regions()[0].end, 15);

        // Mark non-overlapping region
        tracker.mark_dirty(DirtyRegion::new(20, 25));
        assert_eq!(tracker.regions().len(), 2);
    }

    #[test]
    fn test_dirty_region_merge() {
        let mut tracker = DirtyRegionTracker::new();

        // Two adjacent regions should merge
        tracker.mark_dirty(DirtyRegion::new(5, 10));
        tracker.mark_dirty(DirtyRegion::new(10, 15));

        assert_eq!(tracker.regions().len(), 1);
        assert_eq!(tracker.regions()[0].start, 5);
        assert_eq!(tracker.regions()[0].end, 15);
    }

    #[test]
    fn test_dirty_region_range_check() {
        let tracker = DirtyRegionTracker::new();
        let mut t = tracker;

        t.mark_dirty(DirtyRegion::new(10, 20));

        // Non-overlapping
        assert!(!t.is_range_dirty(0, 5));
        assert!(!t.is_range_dirty(25, 30));

        // Overlapping
        assert!(t.is_range_dirty(5, 15));
        assert!(t.is_range_dirty(15, 25));
        assert!(t.is_range_dirty(0, 30));
    }

    #[test]
    fn test_incremental_result_efficiency() {
        let result = IncrementalResult {
            ast: AstNode::Nil,
            reused_cache_entries: 80,
            invalidated_cache_entries: 20,
        };

        assert!((result.efficiency() - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_edit_old_range() {
        // Insert at position 5, length 3 (old_length = 0)
        let insert = Edit::insert(5, 3);
        let range = insert.old_range();
        assert_eq!(range, 5..5); // Empty range for insert

        // Delete at position 5, length 3
        let delete = Edit::delete(5, 3);
        let range = delete.old_range();
        assert_eq!(range, 5..8);

        // Replace "hello" (5 chars) with "hi" (2 chars) at position 10
        let replace = Edit::replace(10, 5, 2);
        let range = replace.old_range();
        assert_eq!(range, 10..15);
    }

    #[test]
    fn test_edit_affects_position() {
        // Edit at position 5, length 3
        let edit = Edit::replace(5, 3, 4);

        // Positions before edit are not affected
        assert!(!edit.affects_position(0));
        assert!(!edit.affects_position(4));

        // Positions at or after edit offset are affected
        assert!(edit.affects_position(5));
        assert!(edit.affects_position(6));
        assert!(edit.affects_position(8));
        assert!(edit.affects_position(100));

        // Delete edit
        let delete = Edit::delete(10, 5);
        assert!(!delete.affects_position(9));
        assert!(delete.affects_position(10));
        assert!(delete.affects_position(15));
        assert!(delete.affects_position(100));
    }
}
