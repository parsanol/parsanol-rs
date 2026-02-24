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

/// A cached parse result
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheEntry {
    /// Position in input
    pub pos: u32,
    /// Atom ID in grammar
    pub atom_id: u16,
    /// Success flag
    pub success: bool,
    /// End position (if success)
    pub end_pos: u32,
    /// Index into arena's node pool (if success)
    pub ast_ref: u32,
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

    /// Statistics
    hits: u64,
    misses: u64,
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

        Self {
            slots: vec![-1i32; capacity],
            entries: Vec::with_capacity(estimated_entries),
            capacity,
            load_factor: 0.75,
            hits: 0,
            misses: 0,
        }
    }

    /// Create a cache sized for a given input length
    #[inline]
    pub fn for_input(input_len: usize, atom_count: usize) -> Self {
        // Estimate: ~1 cache entry per 10 characters * atoms_at_position
        // Typically atoms_at_position is ~2-5, so use factor of 3
        let estimated = (input_len / 10) * atom_count.min(5);
        Self::new(estimated.clamp(1000, 500_000))
    }

    /// Get a cached entry
    #[inline]
    pub fn get(&mut self, pos: u32, atom_id: u16) -> Option<&CacheEntry> {
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
                // Found!
                self.hits += 1;
                return Some(entry);
            }

            // Linear probing
            slot = (slot + 1) & (self.capacity - 1);
        }
    }

    /// Insert an entry into the cache
    #[inline]
    pub fn insert(&mut self, entry: CacheEntry) {
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

    /// Get or insert an entry
    ///
    /// Returns a mutable reference to the entry and whether it was a cache hit.
    #[inline]
    pub fn get_or_insert_with<F>(&mut self, pos: u32, atom_id: u16, f: F) -> (&mut CacheEntry, bool)
    where
        F: FnOnce() -> CacheEntry,
    {
        // First, try to get
        let mut slot = self.hash(pos, atom_id);

        loop {
            let idx = self.slots[slot];

            if idx < 0 {
                // Not found - insert
                break;
            }

            let entry = &self.entries[idx as usize];
            if entry.pos == pos && entry.atom_id == atom_id {
                // Found!
                self.hits += 1;
                return (&mut self.entries[idx as usize], true);
            }

            // Linear probing
            slot = (slot + 1) & (self.capacity - 1);
        }

        // Need to insert
        self.misses += 1;

        if self.entries.len() as f64 / self.capacity as f64 > self.load_factor {
            self.resize();
            // Recompute slot after resize
            slot = self.hash(pos, atom_id);
            while self.slots[slot] >= 0 {
                slot = (slot + 1) & (self.capacity - 1);
            }
        }

        let entry = f();
        let idx = self.entries.len();
        self.entries.push(entry);
        self.slots[slot] = idx as i32;

        (&mut self.entries[idx], false)
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

        // Insert
        cache.insert(CacheEntry {
            pos: 0,
            atom_id: 1,
            success: true,
            end_pos: 5,
            ast_ref: 0,
        });

        // Get
        let entry = cache.get(0, 1);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert!(entry.success);
        assert_eq!(entry.end_pos, 5);
    }

    #[test]
    fn test_collision_handling() {
        let mut cache = DenseCache::new(4); // Small capacity to force collisions

        // Insert multiple entries
        for i in 0..10 {
            cache.insert(CacheEntry {
                pos: i * 100,
                atom_id: (i % 5) as u16,
                success: true,
                end_pos: (i + 1) * 100,
                ast_ref: i,
            });
        }

        // Verify all can be retrieved
        for i in 0..10 {
            let entry = cache.get(i * 100, (i % 5) as u16);
            assert!(entry.is_some(), "Entry {} not found", i);
        }
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = DenseCache::new(16);

        let entry = cache.get(0, 1);
        assert!(entry.is_none());

        let (hits, misses, _) = cache.stats();
        assert_eq!(hits, 0);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_resize() {
        let mut cache = DenseCache::new(4);

        // Insert enough entries to trigger resize
        for i in 0..100 {
            cache.insert(CacheEntry {
                pos: i,
                atom_id: 0,
                success: true,
                end_pos: i + 1,
                ast_ref: i,
            });
        }

        // All entries should still be accessible
        for i in 0..100 {
            let entry = cache.get(i, 0);
            assert!(entry.is_some(), "Entry {} not found after resize", i);
        }
    }

    #[test]
    fn test_get_or_insert() {
        let mut cache = DenseCache::new(16);

        // First call should insert
        let (entry, was_hit) = cache.get_or_insert_with(0, 1, || CacheEntry {
            pos: 0,
            atom_id: 1,
            success: true,
            end_pos: 5,
            ast_ref: 0,
        });
        assert!(!was_hit);
        assert!(entry.success);

        // Second call should hit
        let (entry, was_hit) = cache.get_or_insert_with(0, 1, || CacheEntry {
            pos: 0,
            atom_id: 1,
            success: false,
            end_pos: 0,
            ast_ref: 0,
        });
        assert!(was_hit);
        assert!(entry.success); // Should still have original value
    }

    #[test]
    fn test_clear() {
        let mut cache = DenseCache::new(16);

        cache.insert(CacheEntry {
            pos: 0,
            atom_id: 1,
            success: true,
            end_pos: 5,
            ast_ref: 0,
        });

        assert!(!cache.is_empty());

        cache.clear();

        assert!(cache.is_empty());
        assert!(cache.get(0, 1).is_none());
    }

    #[test]
    fn test_hit_rate() {
        let mut cache = DenseCache::new(16);

        cache.insert(CacheEntry {
            pos: 0,
            atom_id: 1,
            success: true,
            end_pos: 5,
            ast_ref: 0,
        });

        // Hit
        cache.get(0, 1);
        // Miss
        cache.get(1, 1);
        // Hit
        cache.get(0, 1);

        let (hits, misses, hit_rate) = cache.stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
        assert!((hit_rate - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_cache_entry_size() {
        // CacheEntry should be 16 bytes for optimal cache line usage
        // Layout: pos(4) + atom_id(2) + success(1) + padding(1) + end_pos(4) + ast_ref(4) = 16 bytes
        assert_eq!(std::mem::size_of::<CacheEntry>(), 16);
        assert_eq!(std::mem::align_of::<CacheEntry>(), 4);
    }
}
