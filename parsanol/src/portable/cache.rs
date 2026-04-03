//! Dense packrat cache for memoization
//!
//! This module provides a cache-friendly implementation of packrat memoization.
//! Unlike a HashMap, the dense array cache provides:
//!
//! - Better cache locality (linear memory access)
//! - O(1) lookup with linear probing
//! - Lower memory overhead (no pointer chasing)
//! - Predictable performance
//!
//! # Implementation Details
//!
//! The cache uses open addressing with linear probing:
//!
//! 1. **Slots array**: Maps hash to entry index (-1 for empty)
//! 2. **Entries array**: Stores cache entries contiguously
//!
//! Lookups use FNV-1a hash for speed, with linear probing to resolve collisions.
//!
//! # Cache Entry Packing
//!
//! Each `CacheEntry` inlines the AST node data directly (24 bytes), eliminating
//! the need for a separate `cached_nodes: Vec<AstNode>`. The node data consists
//! of a type tag and up to two u32 data fields, covering all node types:
//! Nil, StringRef, InputRef, Array, Hash.

use crate::portable::ast::AstNode;

/// Tag identifying the type of cached AST node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum NodeTag {
    /// Nil/null value
    #[default]
    Nil = 0,
    /// Reference to interned string in arena (data_a = pool_index)
    StringRef = 1,
    /// Reference to original input (data_a = offset, data_b = length)
    InputRef = 2,
    /// Array of child nodes (data_a = pool_index, data_b = length)
    Array = 3,
    /// Hash map (data_a = pool_index, data_b = length)
    Hash = 4,
}

impl NodeTag {
    /// Create from u8, returning None for invalid values
    #[inline]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(NodeTag::Nil),
            1 => Some(NodeTag::StringRef),
            2 => Some(NodeTag::InputRef),
            3 => Some(NodeTag::Array),
            4 => Some(NodeTag::Hash),
            _ => None,
        }
    }
}

/// A cached parse result with inlined node data (24 bytes)
///
/// Instead of storing an index into a separate `cached_nodes: Vec<AstNode>`,
/// the node data is packed directly into the entry. This eliminates the
/// separate Vec (saving ~24 bytes per entry for the Vec element + allocation
/// overhead) and avoids cloning on cache hits.
///
/// Layout: pos(4) + end_pos(4) + data_a(4) + data_b(4) + atom_id(2) + node_tag(1) + success(1) + generation(4) = 24
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CacheEntry {
    /// Position in input
    pub pos: u32,
    /// End position (if success)
    pub end_pos: u32,
    /// Node data field A (pool_index or offset, depending on node_tag)
    pub node_data_a: u32,
    /// Node data field B (length, depending on node_tag)
    pub node_data_b: u32,
    /// Atom ID in grammar
    pub atom_id: u16,
    /// Type of cached node
    pub node_tag: NodeTag,
    /// Whether the parse succeeded
    pub success: bool,
    /// Arena generation at time of cache entry creation
    /// Used to invalidate entries when arena is rolled back
    pub generation: u32,
}

impl Default for CacheEntry {
    fn default() -> Self {
        Self {
            pos: 0,
            end_pos: 0,
            node_data_a: 0,
            node_data_b: 0,
            atom_id: 0,
            node_tag: NodeTag::Nil,
            success: false,
            generation: 0,
        }
    }
}

impl CacheEntry {
    /// Create a new cache entry from an AstNode
    #[inline]
    pub fn from_node(
        pos: u32,
        atom_id: u16,
        end_pos: u32,
        node: &AstNode,
        generation: u32,
    ) -> Self {
        let (tag, data_a, data_b) = match node {
            AstNode::Nil => (NodeTag::Nil, 0, 0),
            AstNode::StringRef { pool_index } => (NodeTag::StringRef, *pool_index, 0),
            AstNode::InputRef { offset, length } => (NodeTag::InputRef, *offset, *length),
            AstNode::Array { pool_index, length } => (NodeTag::Array, *pool_index, *length),
            AstNode::Hash { pool_index, length } => (NodeTag::Hash, *pool_index, *length),
            // Bool, Int, Float shouldn't be cached (scalar types returned directly)
            _ => (NodeTag::Nil, 0, 0),
        };
        Self {
            pos,
            end_pos,
            node_data_a: data_a,
            node_data_b: data_b,
            atom_id,
            node_tag: tag,
            success: true,
            generation,
        }
    }

    /// Create a failed cache entry
    #[inline]
    pub fn failure(pos: u32, atom_id: u16) -> Self {
        Self {
            pos,
            end_pos: pos,
            node_data_a: 0,
            node_data_b: 0,
            atom_id,
            node_tag: NodeTag::Nil,
            success: false,
            generation: 0, // Failures don't reference arena data
        }
    }

    /// Reconstruct an AstNode from the inlined data
    #[inline]
    pub fn to_node(&self) -> AstNode {
        match self.node_tag {
            NodeTag::Nil => AstNode::Nil,
            NodeTag::StringRef => AstNode::StringRef {
                pool_index: self.node_data_a,
            },
            NodeTag::InputRef => AstNode::InputRef {
                offset: self.node_data_a,
                length: self.node_data_b,
            },
            NodeTag::Array => AstNode::Array {
                pool_index: self.node_data_a,
                length: self.node_data_b,
            },
            NodeTag::Hash => AstNode::Hash {
                pool_index: self.node_data_a,
                length: self.node_data_b,
            },
        }
    }
}

/// Dense packrat cache with linear probing
pub struct DenseCache {
    /// Hash table: maps (hash % capacity) -> entry index
    /// -1 means empty slot
    slots: Vec<i32>,

    /// Cache entries (stored contiguously for cache efficiency)
    entries: Vec<CacheEntry>,

    /// Number of slots in the hash table
    capacity: usize,

    /// Load factor threshold (0.0 to 1.0)
    load_factor: f64,

    /// Maximum entries to store (caps memory usage)
    max_entries: usize,

    /// Statistics
    hits: u64,
    misses: u64,

    /// Number of entries dropped due to max_entries cap
    drops: u64,
}

impl Default for DenseCache {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl DenseCache {
    /// Create a new cache with estimated capacity
    #[inline]
    pub fn new(estimated_entries: usize) -> Self {
        // Round up to power of 2 for fast modulo
        let capacity = estimated_entries.next_power_of_two().max(16);
        // Cap entries at capacity * load_factor to prevent unbounded growth
        let max_entries = ((capacity as f64 * 0.75) as usize).max(estimated_entries);

        Self {
            slots: vec![-1i32; capacity],
            entries: Vec::with_capacity(estimated_entries),
            capacity,
            load_factor: 0.75,
            max_entries,
            hits: 0,
            misses: 0,
            drops: 0,
        }
    }

    /// Create a cache sized for a given input length
    ///
    /// # Cache Sizing Strategy
    ///
    /// The cache needs to hold entries for all (position, atom_id) pairs that
    /// might be tried during parsing. With packrat memoization, the worst case
    /// is `input_len * atom_count` entries.
    ///
    /// For small grammars (< 100 atoms), we use a simple heuristic.
    /// For large grammars (like EXPRESS with 2273 atoms), we need a larger cache
    /// to avoid collisions that destroy performance.
    #[inline]
    pub fn for_input(input_len: usize, atom_count: usize) -> Self {
        // For large grammars, we need significantly more cache capacity
        // because each position may try many atoms due to alternatives
        let estimated = if atom_count > 100 {
            // Large grammar: estimate based on actual atom count
            // Not all atoms are tried at every position, so use a fraction
            // but ensure minimum capacity based on atom count
            let base = (input_len as f64 * 0.5) as usize; // ~50% of positions
            let per_pos = (atom_count as f64 * 0.1).ceil() as usize; // ~10% of atoms per pos
            (base * per_pos).max(atom_count * 2) // ensure room for at least 2x atoms
        } else {
            // Small grammar: simple heuristic
            (input_len / 10) * atom_count.max(3)
        };

        // Clamp to reasonable bounds, but with higher minimum for large grammars
        let min_capacity = if atom_count > 100 { 10000 } else { 1000 };
        Self::new(estimated.clamp(min_capacity, 2_000_000))
    }

    /// Get a cached entry
    #[inline]
    pub fn get(&mut self, pos: u32, atom_id: u16, generation: u32) -> Option<&CacheEntry> {
        let mut slot = self.hash(pos, atom_id);

        loop {
            let idx = self.slots[slot];

            if idx < 0 {
                // Empty slot = not found
                self.misses += 1;
                return None;
            }

            let entry = &self.entries[idx as usize];
            if entry.pos == pos && entry.atom_id == atom_id {
                // Check generation to ensure arena hasn't rolled back
                if entry.generation != generation {
                    // Stale entry - arena was rolled back since this was cached
                    self.misses += 1;
                    return None;
                }
                // Found!
                self.hits += 1;
                return Some(entry);
            }

            // Linear probing
            slot = (slot + 1) & (self.capacity - 1);
        }
    }

    /// Insert an entry into the cache
    ///
    /// If the cache is at max_entries, the entry is silently dropped.
    /// This caps memory usage by trading some re-parsing for bounded growth.
    #[inline]
    pub fn insert(&mut self, entry: CacheEntry) {
        // Cap memory: skip insertion if at capacity
        if self.entries.len() >= self.max_entries {
            self.drops += 1;
            return;
        }

        // Check if we need to resize
        if self.entries.len() as f64 / self.capacity as f64 > self.load_factor {
            self.resize();
        }

        let mut slot = self.hash(entry.pos, entry.atom_id);

        // Find empty slot (linear probing)
        while self.slots[slot] >= 0 {
            slot = (slot + 1) & (self.capacity - 1);
        }

        // Insert
        let idx = self.entries.len() as i32;
        self.entries.push(entry);
        self.slots[slot] = idx;
    }

    /// Clear the cache
    #[inline]
    pub fn clear(&mut self) {
        self.slots.fill(-1);
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get cache statistics
    #[inline]
    pub fn stats(&self) -> (u64, u64, f64) {
        let total = self.hits + self.misses;
        let hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
        (self.hits, self.misses, hit_rate)
    }

    /// Get the number of entries
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get memory usage
    #[inline]
    pub fn memory_usage(&self) -> usize {
        self.slots.len() * std::mem::size_of::<i32>()
            + self.entries.capacity() * std::mem::size_of::<CacheEntry>()
    }

    /// Get the capacity (number of slots)
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get an iterator over all entries
    #[inline]
    pub fn entries(&self) -> impl Iterator<Item = &CacheEntry> {
        self.entries.iter()
    }

    /// Get a mutable iterator over all entries
    #[inline]
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut CacheEntry> {
        self.entries.iter_mut()
    }

    /// Retain only entries that match a predicate, rebuilding the hash table
    pub fn retain<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&CacheEntry) -> bool,
    {
        // First, filter entries
        self.entries.retain(|e| predicate(e));

        // Rebuild hash table
        self.slots.fill(-1);
        for (idx, entry) in self.entries.iter().enumerate() {
            let slot = Self::hash_static(entry.pos, entry.atom_id, self.capacity);
            let mut probe = slot;
            while self.slots[probe] >= 0 {
                probe = (probe + 1) & (self.capacity - 1);
            }
            self.slots[probe] = idx as i32;
        }
    }

    /// Compute hash for a position and atom_id
    #[inline]
    pub fn compute_hash(pos: u32, atom_id: u16) -> usize {
        let mut h: u64 = 0x811c9dc5;
        h ^= pos as u64;
        h = h.wrapping_mul(0x01000193);
        h ^= atom_id as u64;
        h = h.wrapping_mul(0x01000193);
        h as usize
    }

    /// Hash function (FNV-1a)
    #[inline]
    fn hash(&self, pos: u32, atom_id: u16) -> usize {
        // FNV-1a hash
        let mut h: u64 = 0x811c9dc5;
        h ^= pos as u64;
        h = h.wrapping_mul(0x01000193);
        h ^= atom_id as u64;
        h = h.wrapping_mul(0x01000193);

        // Use power-of-2 capacity for fast modulo
        (h as usize) & (self.capacity - 1)
    }

    /// Resize the hash table
    fn resize(&mut self) {
        let new_capacity = self.capacity * 2;
        let mut new_slots = vec![-1i32; new_capacity];

        // Rehash all entries
        for (idx, entry) in self.entries.iter().enumerate() {
            let slot = Self::hash_static(entry.pos, entry.atom_id, new_capacity);

            // Find empty slot
            let mut probe_slot = slot;
            while new_slots[probe_slot] >= 0 {
                probe_slot = (probe_slot + 1) & (new_capacity - 1);
            }
            new_slots[probe_slot] = idx as i32;
        }

        self.slots = new_slots;
        self.capacity = new_capacity;
    }

    /// Static hash function for resizing
    #[inline]
    fn hash_static(pos: u32, atom_id: u16, capacity: usize) -> usize {
        let mut h: u64 = 0x811c9dc5;
        h ^= pos as u64;
        h = h.wrapping_mul(0x01000193);
        h ^= atom_id as u64;
        h = h.wrapping_mul(0x01000193);
        (h as usize) & (capacity - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut cache = DenseCache::new(16);

        // Insert a successful entry with InputRef node
        let node = AstNode::InputRef {
            offset: 0,
            length: 5,
        };
        cache.insert(CacheEntry::from_node(0, 1, 5, &node, 0));

        // Get
        let entry = cache.get(0, 1, 0);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert!(entry.success);
        assert_eq!(entry.end_pos, 5);
        assert_eq!(entry.node_tag, NodeTag::InputRef);
        assert_eq!(entry.node_data_a, 0);
        assert_eq!(entry.node_data_b, 5);
    }

    #[test]
    fn test_collision_handling() {
        let mut cache = DenseCache::new(4); // Small capacity to force collisions

        // Insert multiple entries
        for i in 0..10 {
            let node = AstNode::InputRef {
                offset: i * 100,
                length: 100,
            };
            cache.insert(CacheEntry::from_node(
                i * 100,
                (i % 5) as u16,
                (i + 1) * 100,
                &node,
                0,
            ));
        }

        // Verify all can be retrieved
        for i in 0..10 {
            let entry = cache.get(i * 100, (i % 5) as u16, 0);
            assert!(entry.is_some(), "Entry {} not found", i);
        }
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = DenseCache::new(16);

        let entry = cache.get(0, 1, 0);
        assert!(entry.is_none());

        let (hits, misses, _) = cache.stats();
        assert_eq!(hits, 0);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_resize() {
        // Use a large enough estimated size so max_entries doesn't cap inserts
        let mut cache = DenseCache::new(128);

        // Insert enough entries to trigger resize
        for i in 0..100 {
            let node = AstNode::InputRef {
                offset: i,
                length: 1,
            };
            cache.insert(CacheEntry::from_node(i, 0, i + 1, &node, 0));
        }

        // All entries should still be accessible
        for i in 0..100 {
            let entry = cache.get(i, 0, 0);
            assert!(entry.is_some(), "Entry {} not found after resize", i);
        }
    }

    #[test]
    fn test_clear() {
        let mut cache = DenseCache::new(16);

        let node = AstNode::Nil;
        cache.insert(CacheEntry::from_node(0, 1, 5, &node, 0));

        assert!(!cache.is_empty());

        cache.clear();

        assert!(cache.is_empty());
        assert!(cache.get(0, 1, 0).is_none());
    }

    #[test]
    fn test_hit_rate() {
        let mut cache = DenseCache::new(16);

        let node = AstNode::Nil;
        cache.insert(CacheEntry::from_node(0, 1, 5, &node, 0));

        // Hit
        cache.get(0, 1, 0);
        // Miss
        cache.get(1, 1, 0);
        // Hit
        cache.get(0, 1, 0);

        let (hits, misses, hit_rate) = cache.stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
        assert!((hit_rate - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_cache_entry_size() {
        // CacheEntry with inlined node data:
        // pos(4) + end_pos(4) + data_a(4) + data_b(4) + atom_id(2) + tag(1) + success(1) + generation(4) = 24
        assert_eq!(std::mem::size_of::<CacheEntry>(), 24);
        assert_eq!(std::mem::align_of::<CacheEntry>(), 4);
    }

    #[test]
    fn test_node_reconstruction() {
        // Test Nil
        let entry = CacheEntry::from_node(0, 0, 0, &AstNode::Nil, 0);
        assert!(entry.success);
        assert_eq!(entry.node_tag, NodeTag::Nil);
        assert_eq!(entry.to_node(), AstNode::Nil);

        // Test StringRef
        let node = AstNode::StringRef { pool_index: 42 };
        let entry = CacheEntry::from_node(0, 0, 5, &node, 0);
        assert_eq!(entry.to_node(), node);

        // Test InputRef
        let node = AstNode::InputRef {
            offset: 100,
            length: 20,
        };
        let entry = CacheEntry::from_node(0, 0, 120, &node, 0);
        assert_eq!(entry.to_node(), node);

        // Test Array
        let node = AstNode::Array {
            pool_index: 500,
            length: 3,
        };
        let entry = CacheEntry::from_node(0, 0, 0, &node, 0);
        assert_eq!(entry.to_node(), node);

        // Test Hash
        let node = AstNode::Hash {
            pool_index: 1000,
            length: 5,
        };
        let entry = CacheEntry::from_node(0, 0, 0, &node, 0);
        assert_eq!(entry.to_node(), node);
    }

    #[test]
    fn test_failure_entry() {
        let entry = CacheEntry::failure(100, 42);
        assert!(!entry.success);
        assert_eq!(entry.pos, 100);
        assert_eq!(entry.atom_id, 42);
        assert_eq!(entry.node_tag, NodeTag::Nil);
    }
}
