# Research: Incremental Parsing Support

**Status:** Future Research (Priority 4)
**Complexity:** High - Requires significant architectural changes
**Last Updated:** 2026-02-23

---

## Overview

Incremental parsing allows re-parsing only the changed portions of input when modifications occur, dramatically improving performance for interactive use cases like IDEs and text editors.

---

## Problem Statement

When a user edits a document (adds/removes characters), the current parser must re-parse the entire input from scratch. For large files and frequent edits, this becomes a performance bottleneck.

**Goal:** Re-parse only the changed portions and reuse unaffected parse results.

---

## Current Architecture

### Cache Structure

```rust
pub struct CacheEntry {
    pub pos: u32,        // Position in input
    pub atom_id: u16,    // Atom ID in grammar
    pub success: bool,   // Parse success flag
    pub end_pos: u32,    // End position (if success)
    pub ast_ref: u32,    // Index into arena's node pool
}

pub struct DenseCache {
    slots: Vec<i32>,     // Hash table: maps (hash % capacity) -> entry index
    entries: Vec<CacheEntry>,
    capacity: usize,
    // ...
}
```

### Key Challenge

The current cache is **position-based**. When characters are inserted or deleted:
1. All positions after the edit point become invalid
2. The entire cache must be invalidated
3. The parser must re-parse from the edit point

---

## Proposed Architecture

### 1. Offset-Based Invalidation

Track the edit location and invalidate only affected entries:

```rust
pub struct IncrementalCache {
    /// The underlying dense cache
    cache: DenseCache,

    /// Last known input version
    input_version: u64,

    /// Edit regions (for tracking changes)
    edits: Vec<EditRegion>,
}

pub struct EditRegion {
    /// Start position of edit
    start: u32,
    /// Length of deleted text
    deleted_len: u32,
    /// Length of inserted text
    inserted_len: u32,
}

impl IncrementalCache {
    /// Apply an edit and return invalidated regions
    pub fn apply_edit(&mut self, edit: EditRegion) -> Vec<u32> {
        // 1. Find all cache entries that overlap with edit region
        // 2. Invalidate those entries
        // 3. Adjust positions of entries after the edit
        // 4. Return invalidated atom IDs for re-parsing
    }

    /// Adjust positions for entries after an edit
    fn adjust_positions(&mut self, after: u32, delta: i32) {
        // Shift all entries with pos > after by delta
    }
}
```

### 2. Dirty Region Tracking

Track which input regions need re-parsing:

```rust
pub struct DirtyRegionTracker {
    /// Ranges that need re-parsing
    dirty_regions: Vec<Range<u32>>,

    /// Minimum dirty region (for optimization)
    min_dirty: u32,

    /// Maximum dirty region
    max_dirty: u32,
}

impl DirtyRegionTracker {
    /// Mark a region as dirty
    pub fn mark_dirty(&mut self, start: u32, end: u32) { ... }

    /// Check if a position is dirty
    pub fn is_dirty(&self, pos: u32) -> bool { ... }

    /// Get next dirty region to parse
    pub fn next_dirty_region(&self) -> Option<Range<u32>> { ... }
}
```

### 3. Dependency Tracking

Track which rules depend on which input regions:

```rust
pub struct DependencyGraph {
    /// Maps atom ID to the input regions it covers
    atom_regions: HashMap<u16, Vec<Range<u32>>>,

    /// Maps parent atom to child atoms (for propagation)
    dependencies: HashMap<u16, Vec<u16>>,
}

impl DependencyGraph {
    /// When a region is invalidated, find all dependent atoms
    pub fn find_dependents(&self, dirty_region: Range<u32>) -> Vec<u16> { ... }
}
```

---

## Implementation Strategy

### Phase 1: Basic Invalidation

1. **Add edit tracking to cache**
   - Track input version
   - Store last edit position
   - Invalidate entries from edit position onwards

2. **Position adjustment**
   - When characters inserted: shift positions right
   - When characters deleted: shift positions left

3. **Selective re-parsing**
   - Start parsing from dirty region
   - Reuse cached results for clean regions

### Phase 2: Fine-Grained Invalidation

1. **Dependency analysis**
   - Build dependency graph during first parse
   - Use graph to find minimal invalidation set

2. **Region-based caching**
   - Cache results per input region
   - Only invalidate overlapping regions

### Phase 3: Optimization

1. **Incremental AST updates**
   - Update AST nodes in-place where possible
   - Avoid full tree reconstruction

2. **Parallel parsing**
   - Parse dirty regions in parallel
   - Merge results

---

## Challenges

### 1. Left Recursion

In PEG parsers, left recursion requires special handling. Incremental parsing makes this more complex:
- Need to re-evaluate left-recursive rules after edits
- May need to propagate changes up the parse tree

### 2. Complex Edit Patterns

Multiple edits in quick succession:
- Batch edits to minimize invalidation
- Merge overlapping dirty regions

### 3. Memory Management

Arena allocation complicates incremental updates:
- Arena nodes are immutable by design
- May need copy-on-write for modified nodes

### 4. Error Recovery

Partial re-parsing may produce different error locations:
- Need to track error positions
- Update error reporting incrementally

---

## API Design

```rust
use parsanol::portable::{IncrementalParser, Edit};

// Create incremental parser
let mut parser = IncrementalParser::new(&grammar, input)?;

// Initial parse
let ast = parser.parse()?;

// Apply edit
let edit = Edit {
    pos: 100,
    deleted: "old",
    inserted: "new text",
};

// Incremental re-parse
let updated_ast = parser.apply_edit(edit)?;

// Only re-parsed affected regions
println!("Re-parsed {} chars", parser.reparsed_length());
```

---

## Performance Targets

| Metric | Target |
|--------|--------|
| Small edit (1-10 chars) | < 1ms re-parse |
| Medium edit (10-100 chars) | < 10ms re-parse |
| Large edit (> 100 chars) | < 100ms re-parse |
| Cache hit rate after edit | > 80% |

---

## References

- **Tree-sitter**: Incremental parsing system used by Neovim, Atom
- **rust-analyzer**: Incremental parsing for Rust IDE support
- **SwiftSyntax**: Incremental parsing for Swift
- **PEG paper**: Ford, 2004 - "Parsing Expression Grammars"

---

## Research Questions

1. **Dependency granularity**: What's the optimal granularity for dependency tracking?
2. **Arena vs. Tree**: Should we use a different AST representation for incremental parsing?
3. **Memory vs. Speed**: Trade-off between cache size and re-parse speed?
4. **Grammar analysis**: Can we pre-compute dependencies from the grammar?

---

## Conclusion

Incremental parsing is a significant undertaking that requires:
- Cache architecture changes
- Dependency tracking system
- Position adjustment logic
- Careful handling of edge cases

**Recommendation**: Implement after core library is stable and well-tested. Start with simple position-based invalidation, then refine to fine-grained dependencies.

---

## Next Steps

1. Study Tree-sitter's incremental parsing implementation
2. Prototype simple position-based invalidation
3. Benchmark on typical edit patterns
4. Evaluate memory overhead
