# Research: Streaming Parser for Large Inputs

**Status:** Future Research (Priority 4)
**Complexity:** High - Requires major refactoring
**Last Updated:** 2026-02-23

---

## Overview

Streaming parsing enables processing of inputs larger than available memory by parsing in chunks and evicting no-longer-needed data. This is essential for processing large files (logs, data exports, documents) that don't fit in RAM.

---

## Problem Statement

The current parser loads the entire input into memory and keeps the full AST in an arena. For inputs exceeding available RAM:

1. **Memory exhaustion**: Input + arena + cache exceed available memory
2. **Cache pressure**: Packrat cache grows with input size
3. **GC pressure**: Large allocations stress the allocator

**Goal:** Parse inputs larger than memory with bounded memory usage.

---

## Current Architecture

### Memory Usage

```rust
pub struct AstArena {
    // Input reference (full input string)
    input: String,  // O(input_size)

    // Node pool
    nodes: Vec<AstNode>,  // O(parse_tree_size)

    // String pool for interned strings
    string_pool: Vec<String>,  // O(unique_strings)

    // String hash for fast lookup
    string_hash: HashMap<u64, usize>,  // O(unique_strings)
}

pub struct PortableParser<'a> {
    grammar: &'a Grammar,
    input: &'a str,  // Full input reference
    arena: &'a mut AstArena,
    cache: DenseCache,  // O(input_size * atom_count)
}
```

### Key Challenge

PEG parsing requires **backtracking**, which means:
- We may need to access any part of the input at any time
- We may need to revisit earlier parse results
- Cache entries reference arbitrary input positions

---

## Proposed Architecture

### 1. Chunk-Based Input Handling

Divide input into fixed-size chunks:

```rust
pub struct ChunkedInput {
    /// Chunks loaded in memory
    chunks: Vec<InputChunk>,

    /// Maximum chunks to keep in memory
    max_chunks: usize,

    /// LRU eviction policy
    lru: LruCache<usize, ()>,

    /// Total input size (may be larger than loaded chunks)
    total_size: u64,

    /// Source for loading chunks (file, network, etc.)
    source: Box<dyn ChunkSource>,
}

pub struct InputChunk {
    /// Chunk index
    index: usize,

    /// Start position in overall input
    start_pos: u64,

    /// Chunk data
    data: Vec<u8>,

    /// Reference count (for cache entries)
    ref_count: u32,
}

pub trait ChunkSource {
    /// Load a chunk by index
    fn load_chunk(&mut self, index: usize) -> Result<InputChunk, StreamError>;

    /// Get total size
    fn total_size(&self) -> u64;
}
```

### 2. Sliding Window Cache

Limit cache to a sliding window:

```rust
pub struct StreamingCache {
    /// Cache entries within the window
    entries: Vec<CacheEntry>,

    /// Window start position
    window_start: u32,

    /// Window size (bytes)
    window_size: u32,

    /// Entries indexed by relative position
    index: HashMap<(u32, u16), usize>,
}

impl StreamingCache {
    /// Get entry if within window
    pub fn get(&self, pos: u32, atom_id: u16) -> Option<&CacheEntry> {
        if pos < self.window_start || pos >= self.window_start + self.window_size {
            return None;  // Outside window
        }
        // Look up in index...
    }

    /// Slide window forward, evicting old entries
    pub fn slide_window(&mut self, new_start: u32) {
        // Remove entries before new_start
        // Update window_start
    }
}
```

### 3. Streaming Arena

Arena that can evict old nodes:

```rust
pub struct StreamingArena {
    /// Nodes in current window
    nodes: Vec<AstNode>,

    /// String pool with eviction
    string_pool: EvictablePool<String>,

    /// Callback for evicted nodes
    on_evict: Option<Box<dyn Fn(AstNode)>>,
}

impl StreamingArena {
    /// Evict nodes before a position
    pub fn evict_before(&mut self, pos: u32) {
        // Find nodes with all references before pos
        // Call on_evict callback
        // Remove from nodes vector
    }
}
```

### 4. Backtracking Window

Limit backtracking to a window:

```rust
pub struct BacktrackingWindow {
    /// Maximum backtracking distance
    max_backtrack: usize,

    /// Current parse position
    current_pos: usize,

    /// Minimum position we can backtrack to
    min_valid_pos: usize,
}

impl BacktrackingWindow {
    /// Check if backtracking is possible
    pub fn can_backtrack_to(&self, pos: usize) -> bool {
        pos >= self.min_valid_pos
    }

    /// Update window after forward progress
    pub fn advance(&mut self, new_pos: usize) {
        self.current_pos = new_pos;
        self.min_valid_pos = new_pos.saturating_sub(self.max_backtrack);
    }
}
```

---

## Implementation Strategy

### Phase 1: Chunked Input

1. **Implement ChunkedInput**
   - File-based chunk source
   - LRU eviction
   - Position-based chunk lookup

2. **Modify parser to use chunks**
   - Replace `&str` with `ChunkedInput`
   - Handle chunk boundaries

### Phase 2: Sliding Window

1. **Implement StreamingCache**
   - Window-based cache
   - Eviction when window slides

2. **Implement StreamingArena**
   - Node eviction
   - String pool eviction

### Phase 3: Backtracking Limits

1. **Implement BacktrackingWindow**
   - Track valid backtracking range
   - Fail gracefully when exceeded

2. **Grammar annotations**
   - Mark rules that need deep backtracking
   - Warn if limits are exceeded

---

## API Design

```rust
use parsanol::portable::{StreamingParser, FileSource, StreamConfig};

// Configure streaming parser
let config = StreamConfig {
    chunk_size: 64 * 1024,      // 64KB chunks
    max_memory: 100 * 1024 * 1024,  // 100MB max
    max_backtrack: 1024 * 1024, // 1MB backtracking window
};

// Create streaming parser from file
let source = FileSource::open("large_file.txt")?;
let mut parser = StreamingParser::new(&grammar, source, config)?;

// Parse with callback for each top-level rule
parser.parse_streaming(|node| {
    // Process node
    println!("Parsed: {:?}", node);

    // Return true to continue, false to stop
    true
})?;

// Or collect all (if memory allows)
let all_nodes: Vec<AstNode> = parser.collect()?;
```

---

## Challenges

### 1. Backtracking in PEG

PEG parsers may need to backtrack arbitrarily far:
- `a / b` tries `a`, if fails, tries `b`
- Nested alternatives can require deep backtracking

**Solutions:**
- Limit backtracking distance
- Require grammar annotations for deep backtracking rules
- Provide diagnostic when limits hit

### 2. Cache Coherence

When chunks are evicted, cache entries referencing them become invalid:
- Need to track chunk dependencies
- Invalidate cache when chunks evicted

### 3. Input References

AST nodes reference input slices:
- Nodes with InputRef need chunk loaded
- May need to copy strings into arena

### 4. Error Reporting

Error positions may reference evicted chunks:
- Need to preserve error context
- May need to re-load chunks for error display

---

## Alternative: Streaming JSON/File Processing

For structured data (JSON, CSV, logs), a simpler approach:

```rust
use parsanol::portable::{LineParser, StreamingLexer};

// Parse line-by-line
let parser = LineParser::new(&grammar)?;

for line in file.lines() {
    let node = parser.parse_line(&line)?;
    process(node);
}
```

This avoids backtracking issues for line-oriented formats.

---

## Performance Targets

| Metric | Target |
|--------|--------|
| Memory usage | < configured max |
| Chunk load time | < 10ms per chunk |
| Throughput | > 100MB/s on SSD |
| Cache hit rate | > 90% within window |

---

## References

- **tree-sitter**: Streaming parsing for large files
- **serde_json**: Streaming JSON parser
- **nom**: Streaming parser combinators
- **PEG paper**: Ford, 2004 - "Parsing Expression Grammars"

---

## Research Questions

1. **Backtracking limits**: What's a reasonable default for max backtracking?
2. **Chunk size**: Optimal chunk size for SSD vs HDD?
3. **Memory/speed tradeoff**: How much memory for acceptable throughput?
4. **Grammar restrictions**: Can we identify "streamable" grammar subsets?

---

## Conclusion

Streaming parsing requires significant architectural changes:
- Chunk-based input management
- Sliding window cache
- Limited backtracking
- Careful memory management

**Recommendation**: Implement for specific use cases (large logs, JSON streams) before general PEG streaming.

---

## Next Steps

1. Benchmark current parser memory usage on large files
2. Prototype chunked input system
3. Evaluate backtracking patterns in typical grammars
4. Implement line-oriented streaming as simpler alternative
