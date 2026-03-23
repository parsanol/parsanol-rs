//! Thread-safe LRU (Least Recently Used) cache implementation.
//!
//! This provides a bounded cache that automatically evicts the least recently
//! used entries when it reaches capacity, preventing unbounded memory growth.

use hashbrown::HashMap;
use std::collections::VecDeque;

/// A thread-safe bounded LRU cache.
///
/// Maintains insertion order and evicts the least recently used entry
/// when capacity is reached.
pub struct LruCache<K, V> {
    /// Maximum number of entries to cache
    capacity: usize,
    /// The actual cache storage
    map: HashMap<K, V>,
    /// Order tracker - VecDeque maintains LRU order
    order: VecDeque<K>,
}

impl<K: std::hash::Hash + Eq + Clone, V> LruCache<K, V> {
    /// Create a new LRU cache with the given capacity.
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "LRU cache capacity must be > 0");
        Self {
            capacity,
            map: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
        }
    }

    /// Get a value from the cache, marking it as recently used.
    ///
    /// Returns the value if found, or None if not in cache.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            // Move to end (most recently used)
            self.order.retain(|k| k != key);
            self.order.push_back(key.clone());
            self.map.get(key)
        } else {
            None
        }
    }

    /// Insert a value into the cache.
    ///
    /// If the cache is at capacity, evicts the least recently used entry.
    pub fn insert(&mut self, key: K, value: V) {
        // If key exists, remove it first (will re-add at end)
        if self.map.contains_key(&key) {
            self.map.remove(&key);
            self.order.retain(|k| k != &key);
        } else {
            // Evict LRU entry if at capacity
            while self.map.len() >= self.capacity {
                if let Some(lru_key) = self.order.pop_front() {
                    self.map.remove(&lru_key);
                }
            }
        }

        // Insert new entry - clone key for order tracking
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }

    /// Check if the cache contains a key without updating LRU order.
    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Clear all entries from the cache.
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    /// Get the current number of entries in the cache.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Get the maximum capacity of the cache.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_eviction() {
        let mut cache = LruCache::new(3);

        cache.insert(1, "one");
        cache.insert(2, "two");
        cache.insert(3, "three");

        assert_eq!(cache.len(), 3);

        // Adding 4th entry should evict 1
        cache.insert(4, "four");

        assert_eq!(cache.len(), 3);
        assert!(!cache.contains(&1));
        assert!(cache.contains(&2));
        assert!(cache.contains(&3));
        assert!(cache.contains(&4));
    }

    #[test]
    fn test_lru_order() {
        let mut cache = LruCache::new(3);

        cache.insert(1, "one");
        cache.insert(2, "two");
        cache.insert(3, "three");

        // Access 1, making it most recently used
        assert_eq!(cache.get(&1), Some(&"one"));

        // Adding 4 should evict 2 (LRU)
        cache.insert(4, "four");

        assert!(!cache.contains(&2));
        assert!(cache.contains(&1)); // Still there, was accessed
        assert!(cache.contains(&3));
        assert!(cache.contains(&4));
    }

    #[test]
    fn test_update_existing() {
        let mut cache = LruCache::new(3);

        cache.insert(1, "one");
        cache.insert(2, "two");
        cache.insert(3, "three");

        // Update existing key
        cache.insert(1, "ONE");

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get(&1), Some(&"ONE"));
    }

    #[test]
    fn test_clear() {
        let mut cache = LruCache::new(3);

        cache.insert(1, "one");
        cache.insert(2, "two");

        cache.clear();

        assert!(cache.is_empty());
        assert!(!cache.contains(&1));
        assert!(!cache.contains(&2));
    }
}
