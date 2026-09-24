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
        Self {
            regions: Vec::new(),
        }
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
        self.regions
            .binary_search_by(|r| {
                if r.end <= pos {
                    std::cmp::Ordering::Less
                } else if r.start > pos {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
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
    grammar: std::borrow::Cow<'a, Grammar>,

    /// Packrat cache (preserved across parses)
    cache: DenseCache,

    /// Stable arena backing snapshot-marked cache entries: adopted
    /// node data that must survive across parses (TODO.perf/4).
    snapshot_arena: AstArena,

    /// Session-owned persistent output arena for the `*_retained`
    /// parse family (TODO.perf/9): node data from every retained
    /// parse stays valid, so retention keeps full container entries
    /// with no adoption copy and the previous parse TREE persists
    /// alongside the memo window.
    output_arena: AstArena,

    /// Dirty region tracker
    dirty_tracker: DirtyRegionTracker,

    /// Previous input length (for position translation)
    prev_input_len: usize,
}

/// A persistent output arena past this budget is reset wholesale
/// between retained parses (retention dropped, next parse runs cold)
/// instead of growing without bound across an edit session.
const OUTPUT_ARENA_BUDGET: usize = 64 * 1024 * 1024;

impl<'a> IncrementalParser<'a> {
    /// Create a new incremental parser
    #[inline]
    pub fn new(grammar: &'a Grammar) -> Self {
        Self {
            grammar: std::borrow::Cow::Borrowed(grammar),
            cache: DenseCache::new(4096),
            snapshot_arena: AstArena::new(),
            output_arena: AstArena::new(),
            dirty_tracker: DirtyRegionTracker::new(),
            prev_input_len: 0,
        }
    }

    /// Create a new incremental parser owning its grammar (FFI
    /// sessions hold the grammar and the parser together).
    #[inline]
    pub fn owned(grammar: Grammar) -> IncrementalParser<'static> {
        IncrementalParser {
            grammar: std::borrow::Cow::Owned(grammar),
            cache: DenseCache::new(4096),
            snapshot_arena: AstArena::new(),
            output_arena: AstArena::new(),
            dirty_tracker: DirtyRegionTracker::new(),
            prev_input_len: 0,
        }
    }

    /// Parse input for the first time
    pub fn parse(&mut self, input: &str, arena: &mut AstArena) -> Result<AstNode, ParseError> {
        // Clear previous state. The cache is sized for the document,
        // not the 4 KiB default: retention needs the EARLY memo window
        // to survive the whole parse, and the recycling cache discards
        // whatever exceeds capacity (a 4 KiB window holds only the
        // document's tail, so nothing before an edit would ever be
        // retained).
        self.cache = DenseCache::for_input(input.len(), self.grammar.atom_count());
        self.snapshot_arena = AstArena::new();
        self.dirty_tracker.clear();
        self.prev_input_len = 0;

        // Initial parse, then snapshot the whole successful cache so
        // the first incremental parse starts from valid cross-parse
        // entries.
        let (result, _, _) = self.parse_and_snapshot(input, arena);
        result
    }

    /// Parse against the retained cache, then convert the entries the
    /// dirty regions did not touch into snapshots. Returns the tree,
    /// the retained count at parse start, and the post-parse entry
    /// count for the efficiency stats.
    fn parse_and_snapshot(
        &mut self,
        input: &str,
        arena: &mut AstArena,
    ) -> (Result<AstNode, ParseError>, usize, usize) {
        let before = self.cache.len();
        let cutoff = earliest_dirty_offset(&self.dirty_tracker);
        let root_atom = self.grammar.root as u16;
        let input_len_changed = self.prev_input_len != input.len();
        self.prev_input_len = input.len();

        // Drop entries the edit invalidated BEFORE parsing: entries at
        // or after the edit offset describe the OLD input (shifted
        // positions), and replaying them mid-parse produces results
        // beyond the new input's end. STRICT inequality: an entry
        // ending exactly at the edit offset depended on the byte at
        // that offset as its match boundary (maximal runs stop there),
        // and the edit can make the run extend differently.
        //
        // The surviving window is position-stable, so it replays
        // correctly.
        self.cache.retain(|entry| {
            let entry_end = entry.end_pos as usize;
            let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
            entry_end < cutoff && !(input_len_changed && is_root_at_start)
        });

        let mut parser = super::parser::PortableParser::new_with_cache_and_snap(
            &self.grammar,
            input,
            arena,
            std::mem::take(&mut self.cache),
            Some(&self.snapshot_arena),
        );
        let result = parser.parse();
        self.cache = parser.into_cache();
        let post_parse = self.cache.len();

        // Budget guard: a snapshot arena past the cap is dropped
        // wholesale; the next parse runs cold instead of growing
        // without bound across edit sessions.
        if self.snapshot_arena.memory_usage() > 32 * 1024 * 1024 {
            self.snapshot_arena = AstArena::new();
            self.cache.drop_snapshots();
        }

        // Retain only arena-free entries (terminal InputRefs, scalars,
        // failures): adopting pool-backed values into a snapshot arena
        // measured SLOWER than the full re-parse it was meant to avoid
        // (a deep copy of the whole tree per parse); terminals are the
        // expensive byte-matching work, and re-walking structure
        // against terminal hits is cheap. The snapshot MARK stays: it
        // exempts retained entries from the generation check.
        self.cache.retain_snapshot_adopt(
            |entry| {
                let entry_end = entry.end_pos as usize;
                let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
                // Failures are never retained: their validity depends
                // on bytes after the failure position, which an edit
                // can change (an unknowable lookahead distance).
                let arena_free_success = entry.success
                    && matches!(
                        entry.to_node(),
                        crate::portable::ast::AstNode::InputRef { .. }
                            | crate::portable::ast::AstNode::Nil
                    );
                arena_free_success && entry_end < cutoff && !(input_len_changed && is_root_at_start)
            },
            // Never reached: survivors are arena-free by the
            // predicate, and only pool-backed values adopt.
            |node| node.clone(),
        );
        self.dirty_tracker.clear();

        (result, before, post_parse)
    }

    /// Re-parse after an edit
    pub fn parse_with_edit(
        &mut self,
        input: &str,
        arena: &mut AstArena,
        edit: Edit,
    ) -> Result<IncrementalResult, ParseError> {
        self.dirty_tracker.mark_edit(&edit);
        self.finish_parse(input, arena)
    }

    /// Re-parse after multiple edits
    pub fn parse_with_edits(
        &mut self,
        input: &str,
        arena: &mut AstArena,
        edits: &[Edit],
    ) -> Result<IncrementalResult, ParseError> {
        for edit in edits {
            self.dirty_tracker.mark_edit(edit);
        }
        self.finish_parse(input, arena)
    }

    /// Parse with the current dirty regions, then retain the cache
    /// window that is provably unaffected by them as snapshots.
    ///
    /// Retention rule: an entry survives only when its result ended
    /// at or before the earliest edit offset (its substring is
    /// byte-identical in the new input), and - when the input length
    /// changed - the root entry at position 0 is dropped, because the
    /// root must consume the whole (new) input. Surviving pool-backed
    /// node data is adopted into the snapshot arena; a fresh parse's
    /// arena is a different one, so unadopted pool references would
    /// dangle (the pre-snapshot implementation silently produced
    /// wrong trees across parses).
    fn finish_parse(
        &mut self,
        input: &str,
        arena: &mut AstArena,
    ) -> Result<IncrementalResult, ParseError> {
        let (result, _retained_before, post_parse) = self.parse_and_snapshot(input, arena);
        Ok(IncrementalResult {
            ast: result?,
            reused_cache_entries: self.cache.len(),
            invalidated_cache_entries: post_parse - self.cache.len(),
        })
    }

    // ========================================================================
    // Retained-tree parse family (TODO.perf/9)
    //
    // The `*_retained` parses own their output arena: node data from
    // every retained parse stays valid until the NEXT retained parse
    // or `clear`, so retention keeps FULL container entries (Array /
    // Hash / StringRef) with an identity "adoption" — no cross-arena
    // copy. This removes the adoption-cost ceiling that capped the
    // snapshot tier at terminal entries (TODO.perf/4/8) and persists
    // the previous parse tree alongside the memo window, which the
    // prefix-splice design builds on.
    // ========================================================================

    /// Parse input for the first time into the session-owned arena.
    ///
    /// The returned [`AstNode`] indexes [`Self::retained_arena`] and
    /// remains valid until the next `parse*_retained` call or
    /// [`Self::clear`]. Unlike [`Self::parse`], the tree is NOT
    /// rebuilt into a caller-provided arena — that is what lets the
    /// memo retain whole subtrees across edits.
    pub fn parse_retained(&mut self, input: &str) -> Result<AstNode, ParseError> {
        let (result, _, _) = self.retained_parse(input, false);
        result
    }

    /// Re-parse after an edit, into the session-owned arena.
    pub fn parse_with_edit_retained(
        &mut self,
        input: &str,
        edit: Edit,
    ) -> Result<IncrementalResult, ParseError> {
        self.dirty_tracker.mark_edit(&edit);
        self.finish_retained(input)
    }

    /// Re-parse after multiple edits, into the session-owned arena.
    pub fn parse_with_edits_retained(
        &mut self,
        input: &str,
        edits: &[Edit],
    ) -> Result<IncrementalResult, ParseError> {
        for edit in edits {
            self.dirty_tracker.mark_edit(edit);
        }
        self.finish_retained(input)
    }

    /// The arena the retained parse family builds into. Nodes from the
    /// most recent `parse*_retained` call index this arena.
    #[inline]
    pub fn retained_arena(&self) -> &AstArena {
        &self.output_arena
    }

    /// The shared engine of the retained family: cold-start reset when
    /// `fresh`, otherwise parse against the retained cache window.
    fn retained_parse(
        &mut self,
        input: &str,
        fresh: bool,
    ) -> (Result<AstNode, ParseError>, usize, usize) {
        // Budget guard first: a persistent output arena past the cap
        // is reset wholesale (retention with it) instead of growing
        // without bound across an edit session. Resetting BEFORE the
        // parse keeps the about-to-be-built tree valid.
        if self.output_arena.memory_usage() > OUTPUT_ARENA_BUDGET {
            self.output_arena = AstArena::new();
            self.cache.clear();
        }

        let before = self.cache.len();
        let cutoff = if fresh {
            // First (or cold) parse: no edit invalidated anything.
            self.cache = DenseCache::for_input(input.len(), self.grammar.atom_count());
            self.dirty_tracker.clear();
            self.prev_input_len = 0;
            usize::MAX
        } else {
            earliest_dirty_offset(&self.dirty_tracker)
        };
        let root_atom = self.grammar.root as u16;
        let input_len_changed = self.prev_input_len != input.len();
        self.prev_input_len = input.len();

        // Same pre-parse invalidation as the snapshot tier.
        self.cache.retain(|entry| {
            let entry_end = entry.end_pos as usize;
            let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
            entry_end < cutoff && !(input_len_changed && is_root_at_start)
        });

        self.output_arena.set_input(input.to_owned());
        let mut parser = super::parser::PortableParser::new_with_cache_in_place(
            &self.grammar,
            input,
            &mut self.output_arena,
            std::mem::take(&mut self.cache),
        );
        let result = parser.parse();
        self.cache = parser.into_cache();
        let post_parse = self.cache.len();

        // Retention keeps every success fully before the edit cutoff:
        // the arena is persistent, so pool-backed container entries
        // need no adoption (an identity "adopt" just marks them
        // snapshot, which exempts them from arena-generation checks).
        // Failures stay never-retained, exactly like the snapshot
        // tier.
        self.cache.retain_snapshot_adopt(
            |entry| {
                let entry_end = entry.end_pos as usize;
                let is_root_at_start = entry.pos == 0 && entry.atom_id == root_atom;
                entry.success && entry_end < cutoff && !(input_len_changed && is_root_at_start)
            },
            |node| node.clone(),
        );
        self.dirty_tracker.clear();

        (result, before, post_parse)
    }

    /// Retained variant of [`Self::finish_parse`].
    fn finish_retained(&mut self, input: &str) -> Result<IncrementalResult, ParseError> {
        let (result, _retained_before, post_parse) = self.retained_parse(input, false);
        Ok(IncrementalResult {
            ast: result?,
            reused_cache_entries: self.cache.len(),
            invalidated_cache_entries: post_parse - self.cache.len(),
        })
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
        self.snapshot_arena = AstArena::new();
        self.output_arena = AstArena::new();
        self.dirty_tracker.clear();
        self.prev_input_len = 0;
    }
}

/// Earliest dirty-region start, or `usize::MAX` when nothing is
/// dirty: the retention cutoff every retained entry must end before.
fn earliest_dirty_offset(tracker: &DirtyRegionTracker) -> usize {
    tracker
        .regions()
        .iter()
        .map(|r| r.start)
        .min()
        .unwrap_or(usize::MAX)
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

        assert_eq!(edit.translate_position(0), 0); // Before edit: unchanged
        assert_eq!(edit.translate_position(4), 4); // Before edit: unchanged
        assert_eq!(edit.translate_position(5), 5); // At edit start: unchanged
        assert_eq!(edit.translate_position(6), 9); // After edit: shifted by delta
        assert_eq!(edit.translate_position(10), 13); // After edit: shifted by delta

        // Delete at position 5, length 3
        // Old: positions 0-9, New: positions 0-6 (deleted 3 chars at pos 5-7)
        let edit = Edit::delete(5, 3);

        assert_eq!(edit.translate_position(0), 0); // Before edit: unchanged
        assert_eq!(edit.translate_position(4), 4); // Before edit: unchanged
        assert_eq!(edit.translate_position(5), 5); // At edit start: maps to new position 5
        assert_eq!(edit.translate_position(6), 5); // Inside deleted: maps to position 5
        assert_eq!(edit.translate_position(7), 5); // Inside deleted: maps to position 5
        assert_eq!(edit.translate_position(8), 5); // At end of deleted: maps to position 5
        assert_eq!(edit.translate_position(9), 6); // After edit: shifted by delta
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

#[cfg(test)]
mod edit_sequence_tests {
    use super::*;
    use crate::portable::arena::AstArena;
    use crate::portable::grammar::{Atom, Grammar};
    use crate::portable::parser::PortableParser;

    /// Line-based "key=value" grammar: repetition of
    /// seq(key, "=", value, newline).
    fn kv_grammar() -> Grammar {
        let mut g = Grammar::new();
        let key = g.add_atom(Atom::Re {
            pattern: "[a-z][a-z0-9]*".to_string(),
        });
        let val = g.add_atom(Atom::Re {
            pattern: "[0-9]+".to_string(),
        });
        let eq = g.add_atom(Atom::Str {
            pattern: "=".to_string(),
        });
        let nl = g.add_atom(Atom::Str {
            pattern: "\n".to_string(),
        });
        let pair = g.add_atom(Atom::Sequence {
            atoms: vec![key, eq, val, nl],
        });
        let root = g.add_atom(Atom::Repetition {
            atom: pair,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        g.root = root;
        g
    }

    fn doc(lines: usize) -> String {
        (0..lines)
            .map(|i| format!("key{}={}\n", i, i * 7))
            .collect()
    }

    fn walk_strings(node: &AstNode, arena: &AstArena, input: &str, out: &mut Vec<String>) {
        match node {
            AstNode::InputRef { offset, length } => {
                out.push(input[*offset as usize..*offset as usize + *length as usize].to_string())
            }
            AstNode::Array { pool_index, length } => {
                for child in arena.get_array(*pool_index as usize, *length as usize) {
                    walk_strings(&child, arena, input, out);
                }
            }
            AstNode::StringRef { pool_index } => {
                out.push(arena.get_string(*pool_index as usize).to_string())
            }
            _ => out.push(format!("{node:?}")),
        }
    }

    /// Full parse flattened to a logical string list (arena indices
    /// differ between parses; the logical tree must not).
    fn full_parse(grammar: &Grammar, input: &str) -> Option<Vec<String>> {
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(grammar, input, &mut arena);
        match parser.parse() {
            Ok(tree) => {
                let mut out = Vec::new();
                walk_strings(&tree, &arena, input, &mut out);
                Some(out)
            }
            Err(_) => None,
        }
    }

    fn flatten(node: &AstNode, arena: &AstArena, input: &str) -> Vec<String> {
        let mut out = Vec::new();
        walk_strings(node, arena, input, &mut out);
        out
    }

    /// TODO.perf/4 gate: incremental re-parses after a deterministic
    /// edit sequence must produce the same trees as full re-parses,
    /// and edits late in the document must reuse the early cache
    /// window (the whole point of the retention rule).
    #[test]
    fn incremental_matches_full_reparse_on_edit_sequences() {
        let grammar = kv_grammar();
        let mut input = doc(1200);

        let mut inc = IncrementalParser::owned(grammar.clone());
        let mut arena = AstArena::new();
        let initial = inc.parse(&input, &mut arena).expect("initial parse");
        let reference = full_parse(&grammar, &input).expect("reference parse");
        assert_eq!(flatten(&initial, &arena, &input), reference);

        // Deterministic pseudo-random edit sequence (LCG), biased to
        // line boundaries so intermediate documents stay parseable.
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };

        for step in 0..30 {
            let line = next() % input.lines().count().max(1);
            let offset = input
                .lines()
                .take(line)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .min(input.len());
            let edit = match next() % 3 {
                0 => {
                    // insert a line before `offset`
                    let text = format!("added{}={}\n", step, step);
                    input.insert_str(offset.min(input.len()), &text);
                    Edit::insert(offset, text.len())
                }
                1 => {
                    // delete one full line if possible
                    let rest = &input[offset.min(input.len())..];
                    let len = rest.find('\n').map(|i| i + 1).unwrap_or(0);
                    if len == 0 {
                        continue;
                    }
                    input.replace_range(offset..offset + len, "");
                    Edit::delete(offset, len)
                }
                _ => {
                    // replace a line's value digits
                    let rest = &input[offset.min(input.len())..];
                    let len = rest.find('\n').unwrap_or(rest.len());
                    if len == 0 {
                        continue;
                    }
                    let mut replacement = format!("key{}={}", next() % 1000, next() % 100000);
                    replacement.push('\n');
                    let range = offset..offset + len;
                    replacement.truncate(len);
                    input.replace_range(range.clone(), &replacement);
                    Edit::replace(offset, len, replacement.len())
                }
            };

            // Acceptance must agree: the incremental parse fails
            // exactly when a full re-parse would.
            let reference = full_parse(&grammar, &input);
            let mut arena = AstArena::new();
            let result = inc.parse_with_edit(&input, &mut arena, edit);
            match (reference, result) {
                (Some(flat), Ok(r)) => assert_eq!(
                    flatten(&r.ast, &arena, &input),
                    flat,
                    "tree mismatch at edit step {step} (line {line})"
                ),
                (None, Err(_)) => {}
                (f, r) => panic!(
                    "acceptance mismatch at step {step}: full_ok={} inc_ok={} err={r:?}",
                    f.is_some(),
                    r.is_ok()
                ),
            }
        }
    }

    /// The Ruby serializer emits named rules (Entity/Named atoms); a
    /// rule-referenced body exercises rule-boundary memo entries the
    /// flat grammar above does not.
    #[test]
    fn rule_based_grammar_edit_matches_full_parse() {
        let mut g = Grammar::new();
        let key = g.add_atom(Atom::Re {
            pattern: "[a-z][a-z0-9]*".to_string(),
        });
        let val = g.add_atom(Atom::Re {
            pattern: "[0-9]+".to_string(),
        });
        let eq = g.add_atom(Atom::Str {
            pattern: "=".to_string(),
        });
        let nl = g.add_atom(Atom::Str {
            pattern: "\n".to_string(),
        });
        let pair_body = g.add_atom(Atom::Sequence {
            atoms: vec![key, eq, val, nl],
        });
        let pair_rule = g.add_atom(Atom::Named {
            name: "pair".to_string(),
            atom: pair_body,
        });
        let root = g.add_atom(Atom::Repetition {
            atom: pair_rule,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        g.root = root;

        let doc: String = (1..200).map(|i| format!("key{}={}\n", i, i * 3)).collect();
        let mut inc = IncrementalParser::owned(g.clone());
        let mut arena = AstArena::new();
        inc.parse(&doc, &mut arena).expect("initial");

        let target = "key50=150\n";
        let offset = doc.find(target).expect("line");
        let edited = doc.replacen(target, "zed=9\n", 1);
        let mut arena = AstArena::new();
        let result = inc.parse_with_edit(
            &edited,
            &mut arena,
            Edit::replace(offset, target.len(), "zed=9\n".len()),
        );
        assert!(
            result.is_ok(),
            "rule-based incremental parse failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn late_edits_reuse_the_early_cache_window() {
        let grammar = kv_grammar();
        let input = doc(3000);
        let mut inc = IncrementalParser::owned(grammar);
        let mut arena = AstArena::new();
        inc.parse(&input, &mut arena).expect("initial parse");

        // One edit near the end: everything before it is reusable.
        let last_line_start = input.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let last_len = input.len() - last_line_start;
        let edited = format!("{}zed=1\n", &input[..last_line_start]);
        let mut arena = AstArena::new();
        let result = inc
            .parse_with_edit(
                &edited,
                &mut arena,
                Edit::replace(last_line_start, last_len, 7),
            )
            .expect("incremental parse");
        assert!(
            result.reused_cache_entries > 100,
            "expected substantial reuse, got {} reused / {} invalidated",
            result.reused_cache_entries,
            result.invalidated_cache_entries
        );
    }
}

#[cfg(test)]
mod dynamic_session_tests {
    use super::*;
    use crate::portable::arena::AstArena;
    use crate::portable::dynamic::{register_dynamic_callback, DynamicCallback, DynamicContext};
    use crate::portable::grammar::{Atom, Grammar};

    struct PassthroughCallback;

    impl DynamicCallback for PassthroughCallback {
        fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
            Some(Atom::Str {
                pattern: "k=1\n".to_string(),
            })
        }
        fn description(&self) -> &str {
            "passthrough"
        }
    }

    /// Dynamic grammars are eligible for incremental sessions
    /// (parsanol-ruby#80 item 3): the walker's dynamic-dependent memo
    /// filter keeps retention sound, and a dynamic line's edit
    /// re-parses to the same tree as a full parse.
    #[test]
    fn dynamic_grammar_session_edit_matches_full_parse() {
        let cb_id = register_dynamic_callback(Box::new(PassthroughCallback));
        let mut g = Grammar::new();
        let dyn_line = g.add_atom(Atom::Dynamic { callback_id: cb_id });
        let rep = g.add_atom(Atom::Repetition {
            atom: dyn_line,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        g.root = rep;

        let doc = "k=1\nk=1\nk=1\n";
        let edited = "k=1\nk=1\n";

        let mut inc = IncrementalParser::owned(g.clone());
        let mut arena = AstArena::new();
        inc.parse(doc, &mut arena).expect("initial dynamic parse");

        let mut arena = AstArena::new();
        let result = inc
            .parse_with_edit(edited, &mut arena, Edit::delete(8, 4))
            .expect("dynamic incremental parse");

        // Tree parity with a full parse of the edited input.
        let mut arena2 = AstArena::new();
        let mut full = crate::portable::parser::PortableParser::new(&g, edited, &mut arena2);
        let full_tree = full.parse().expect("full parse");
        assert_eq!(
            format!("{:?}", result.ast),
            format!("{full_tree:?}"),
            "arena-free comparison: both trees are InputRef/Array shapes"
        );
    }
}
