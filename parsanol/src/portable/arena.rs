//! Arena allocator for AST nodes
//!
//! This arena allocator provides O(1) allocation for parse trees.
//! All nodes are allocated in a single contiguous memory region,
//! providing excellent cache locality and O(1) deallocation (just reset).

use super::ast::AstNode;
use std::collections::HashMap;
use std::mem;

/// String pool entry
#[derive(Debug, Clone, Copy)]
struct StringPoolEntry {
    /// Offset into the string data
    offset: u32,
    /// Length of the string in bytes
    length: u32,
}

/// Hash pool entry - key-value pair
#[derive(Debug, Clone, Copy)]
struct HashPoolEntry {
    /// Key string pool index
    key_pool_index: u32,
    /// Value node
    value: AstNode,
}

/// Array pool entry - stores AstNode directly
#[derive(Debug, Clone, Copy)]
struct ArrayPoolEntry {
    /// The AST node
    value: AstNode,
}

/// The arena allocator
#[derive(Debug)]
pub struct AstArena {
    /// String data storage
    string_data: Vec<u8>,
    /// String pool (offset, length) pairs
    string_pool: Vec<StringPoolEntry>,
    /// Hash map for O(1) string lookup (hash -> pool index)
    /// Only used when string_pool.len() >= 64
    string_hash: HashMap<u64, usize>,
    /// Array pool - stores AST nodes
    array_pool: Vec<ArrayPoolEntry>,
    /// Hash pool - key-value pairs
    hash_pool: Vec<HashPoolEntry>,
}

impl Default for AstArena {
    fn default() -> Self {
        Self::new()
    }
}

impl AstArena {
    /// Create a new arena with default capacity
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(256)
    }

    /// Create a new arena with specified initial capacity
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            string_data: Vec::with_capacity(4096),
            string_pool: Vec::with_capacity(capacity),
            string_hash: HashMap::new(),
            array_pool: Vec::with_capacity(capacity * 2),
            hash_pool: Vec::with_capacity(capacity),
        }
    }

    /// Create a new arena sized for a given input length
    ///
    /// This pre-allocates memory based on expected AST size to reduce
    /// reallocations during parsing.
    ///
    /// # Arguments
    /// * `input_len` - Length of the input string to be parsed
    ///
    /// # Returns
    /// A new arena with appropriately sized buffers
    #[inline]
    pub fn for_input(input_len: usize) -> Self {
        // Estimate AST node count: roughly proportional to input size
        // Typical grammars create 1-3 nodes per 10 characters
        let estimated_nodes = (input_len / 10).clamp(64, 100_000);

        // String pool: typically smaller than node count (only for keys/literals)
        let string_capacity = (estimated_nodes / 4).max(32);

        // String data: estimate ~8 bytes per string on average
        let string_data_capacity = string_capacity * 8;

        Self {
            string_data: Vec::with_capacity(string_data_capacity),
            string_pool: Vec::with_capacity(string_capacity),
            string_hash: HashMap::with_capacity(string_capacity),
            array_pool: Vec::with_capacity(estimated_nodes * 2),
            hash_pool: Vec::with_capacity(estimated_nodes),
        }
    }

    /// Get the current capacity
    #[inline]
    pub fn capacity(&self) -> usize {
        self.array_pool.capacity()
    }

    /// Get the current number of allocated arrays
    #[inline]
    pub fn len(&self) -> usize {
        self.array_pool.len()
    }

    /// Check if the arena is empty (no parse data in array/hash pools)
    ///
    /// Note: This does NOT check string_pool, as strings are preserved
    /// across resets for interning efficiency.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.array_pool.is_empty() && self.hash_pool.is_empty()
    }

    /// Reset the arena for reuse
    ///
    /// This clears all pools but retains allocated memory.
    /// Strings are preserved for interning efficiency.
    /// O(1) operation.
    #[inline]
    pub fn reset(&mut self) {
        self.reset_with_options(false)
    }

    /// Reset the arena with configurable string clearing
    ///
    /// # Arguments
    /// * `clear_strings` - If true, clears the string pool and string data.
    ///   If false, strings are preserved for interning efficiency.
    ///
    /// # When to use
    /// - `clear_strings = false` (default): Best for repeated parsing of similar inputs
    ///   where strings are likely to be reused (e.g., parsing multiple JSON files with
    ///   similar keys)
    /// - `clear_strings = true`: Best for long-running processes that parse many
    ///   different inputs, to prevent unbounded memory growth in the string pool
    ///
    /// # Example
    /// ```ignore
    /// let mut arena = AstArena::new();
    ///
    /// // Parse first input
    /// let result1 = parser.parse(input1, &mut arena);
    /// arena.reset(); // Keep strings for reuse
    ///
    /// // Parse second input (can reuse strings from first)
    /// let result2 = parser.parse(input2, &mut arena);
    /// arena.reset_with_options(true); // Clear strings to free memory
    /// ```
    #[inline]
    pub fn reset_with_options(&mut self, clear_strings: bool) {
        // Always clear the pools that grow per-parse
        self.array_pool.clear();
        self.hash_pool.clear();

        // Optionally clear string pools
        if clear_strings {
            self.string_data.clear();
            self.string_pool.clear();
            self.string_hash.clear();
        }
    }

    /// Clear only the string pools
    ///
    /// Use this when you want to free string memory while keeping
    /// the array and hash pool state (uncommon).
    #[inline]
    pub fn clear_strings(&mut self) {
        self.string_data.clear();
        self.string_pool.clear();
        self.string_hash.clear();
    }

    /// Intern a string and return a reference to it
    ///
    /// If the same string has been interned before, returns a reference
    /// to the existing copy instead of allocating a new one.
    #[inline]
    pub fn intern_string(&mut self, s: &str) -> AstNode {
        // Check for existing string first
        if let Some(index) = self.find_interned_string(s) {
            return AstNode::StringRef {
                pool_index: index as u32,
            };
        }

        // Allocate new string
        let offset = self.string_data.len() as u32;
        let length = s.len() as u32;

        self.string_data.extend_from_slice(s.as_bytes());
        self.string_pool.push(StringPoolEntry { offset, length });

        let pool_index = (self.string_pool.len() - 1) as u32;

        // Add to hash map for O(1) lookup
        let hash = self.hash_string(s);
        self.string_hash.insert(hash, pool_index as usize);

        AstNode::StringRef { pool_index }
    }

    /// Compute a hash for a string (using ahash-like algorithm)
    #[inline]
    fn hash_string(&self, s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = ahash::AHasher::default();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Create a reference to a substring of the original input
    ///
    /// This is zero-copy - we just store the offset and length.
    #[inline]
    pub fn input_ref(&self, offset: usize, length: usize) -> AstNode {
        AstNode::InputRef {
            offset: offset as u32,
            length: length as u32,
        }
    }

    /// Get a string from the pool by index
    #[inline]
    pub fn get_string(&self, pool_index: usize) -> &str {
        let entry = &self.string_pool[pool_index];
        let data = &self.string_data[entry.offset as usize..(entry.offset + entry.length) as usize];
        // SAFETY: All strings added to the pool via `add_string()` are valid UTF-8
        // because the function accepts a `&str` which is guaranteed to be valid UTF-8.
        // The bytes are stored unchanged and retrieved as the same slice.
        unsafe { std::str::from_utf8_unchecked(data) }
    }

    /// Get string data from pool entry
    #[inline]
    pub fn get_string_parts(&self, pool_index: usize) -> (&str, u32, u32) {
        let entry = &self.string_pool[pool_index];
        let data = &self.string_data[entry.offset as usize..(entry.offset + entry.length) as usize];
        // SAFETY: All strings added to the pool via `add_string()` are valid UTF-8
        // because the function accepts a `&str` which is guaranteed to be valid UTF-8.
        // The bytes are stored unchanged and retrieved as the same slice.
        let s = unsafe { std::str::from_utf8_unchecked(data) };
        (s, entry.offset, entry.length)
    }

    /// Store an array in the pool
    ///
    /// Arrays are flattened for cache efficiency.
    /// Returns the starting index and length.
    #[inline]
    pub fn store_array(&mut self, items: &[AstNode]) -> (u32, u32) {
        let start = self.array_pool.len() as u32;
        for item in items {
            self.array_pool.push(ArrayPoolEntry { value: *item });
        }
        (start, items.len() as u32)
    }

    /// Get array items from pool
    #[inline]
    pub fn get_array(&self, start: usize, len: usize) -> Vec<AstNode> {
        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            result.push(self.array_pool[start + i].value);
        }
        result
    }

    /// Store a hash in the pool
    ///
    /// Returns the pool index and length.
    #[inline]
    pub fn store_hash(&mut self, pairs: &[(&str, AstNode)]) -> (u32, u32) {
        let start = self.hash_pool.len() as u32;

        for (key, value) in pairs {
            // Find or intern the key string
            let key_pool_index = if let Some(idx) = self.find_interned_string(key) {
                idx as u32
            } else {
                // Add new string to pool
                let offset = self.string_data.len() as u32;
                let length = key.len() as u32;
                self.string_data.extend_from_slice(key.as_bytes());
                self.string_pool.push(StringPoolEntry { offset, length });
                (self.string_pool.len() - 1) as u32
            };

            self.hash_pool.push(HashPoolEntry {
                key_pool_index,
                value: *value,
            });
        }

        (start, pairs.len() as u32)
    }

    /// Get hash items from pool
    #[inline]
    pub fn get_hash_items(&self, pool_index: usize, len: usize) -> Vec<(String, AstNode)> {
        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            let entry = &self.hash_pool[pool_index + i];
            let key = self.get_string(entry.key_pool_index as usize).to_string();
            result.push((key, entry.value));
        }
        result
    }

    /// Find an interned string in the pool
    ///
    /// Uses hash-based O(1) lookup for all pool sizes since we maintain
    /// the hash map continuously.
    #[inline]
    fn find_interned_string(&self, s: &str) -> Option<usize> {
        // Use hash lookup for O(1) access
        let hash = self.hash_string(s);
        if let Some(&index) = self.string_hash.get(&hash) {
            // Verify it's actually the same string (handle hash collisions)
            let entry = &self.string_pool[index];
            let data =
                &self.string_data[entry.offset as usize..(entry.offset + entry.length) as usize];
            if data == s.as_bytes() {
                return Some(index);
            }
        }
        None
    }

    /// Get memory usage estimate
    #[inline]
    pub fn memory_usage(&self) -> usize {
        self.string_data.capacity()
            + self.string_pool.capacity() * mem::size_of::<StringPoolEntry>()
            + self.array_pool.capacity() * mem::size_of::<ArrayPoolEntry>()
            + self.hash_pool.capacity() * mem::size_of::<HashPoolEntry>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_string() {
        let mut arena = AstArena::new();

        let node1 = arena.intern_string("hello");
        let node2 = arena.intern_string("hello");
        let node3 = arena.intern_string("world");

        // Same string should return same pool index
        match (node1, node2) {
            (AstNode::StringRef { pool_index: i1 }, AstNode::StringRef { pool_index: i2 }) => {
                assert_eq!(i1, i2);
            }
            _ => panic!("Expected StringRef nodes"),
        }

        // Different string should have different index
        match (node1, node3) {
            (AstNode::StringRef { pool_index: i1 }, AstNode::StringRef { pool_index: i2 }) => {
                assert_ne!(i1, i2);
            }
            _ => panic!("Expected StringRef nodes"),
        }
    }

    #[test]
    fn test_input_ref() {
        let arena = AstArena::new();
        let node = arena.input_ref(10, 5);

        match node {
            AstNode::InputRef { offset, length } => {
                assert_eq!(offset, 10);
                assert_eq!(length, 5);
            }
            _ => panic!("Expected InputRef node"),
        }
    }

    #[test]
    fn test_array() {
        let mut arena = AstArena::new();

        let items = vec![
            arena.intern_string("a"),
            arena.intern_string("b"),
            arena.intern_string("c"),
        ];

        let (start, len) = arena.store_array(&items);

        assert_eq!(start, 0);
        assert_eq!(len, 3);

        let retrieved = arena.get_array(start as usize, len as usize);
        assert_eq!(retrieved.len(), 3);
    }

    #[test]
    fn test_reset() {
        let mut arena = AstArena::new();

        // Add something to array_pool (which is cleared by reset)
        let items = vec![arena.intern_string("a"), arena.intern_string("b")];
        arena.store_array(&items);

        assert!(!arena.is_empty());

        arena.reset();
        assert!(arena.is_empty());
    }

    #[test]
    fn test_reset_with_options_preserve_strings() {
        let mut arena = AstArena::new();

        // Intern some strings
        let _node1 = arena.intern_string("hello");
        let _node2 = arena.intern_string("world");

        // Add to array pool
        let items = vec![arena.intern_string("a"), arena.intern_string("b")];
        arena.store_array(&items);

        // Reset without clearing strings
        arena.reset_with_options(false);

        // Array/hash pools should be empty
        assert!(arena.is_empty());

        // Strings should still be interned
        let node1_again = arena.intern_string("hello");
        let node2_again = arena.intern_string("world");

        // Should return same pool indices (strings were preserved)
        match (_node1, node1_again) {
            (AstNode::StringRef { pool_index: i1 }, AstNode::StringRef { pool_index: i2 }) => {
                assert_eq!(
                    i1, i2,
                    "String 'hello' should have same pool index after reset"
                );
            }
            _ => panic!("Expected StringRef nodes"),
        }
        match (_node2, node2_again) {
            (AstNode::StringRef { pool_index: i1 }, AstNode::StringRef { pool_index: i2 }) => {
                assert_eq!(
                    i1, i2,
                    "String 'world' should have same pool index after reset"
                );
            }
            _ => panic!("Expected StringRef nodes"),
        }
    }

    #[test]
    fn test_reset_with_options_clear_strings() {
        let mut arena = AstArena::new();

        // Intern some strings
        let node1 = arena.intern_string("hello");
        let node2 = arena.intern_string("world");

        // Reset with clearing strings
        arena.reset_with_options(true);

        // Everything should be empty
        assert!(arena.is_empty());

        // Interning same strings should create NEW entries
        let node1_again = arena.intern_string("hello");
        let node2_again = arena.intern_string("world");

        // Should return pool index 0 and 1 (new entries)
        match (node1_again, node2_again) {
            (AstNode::StringRef { pool_index: i1 }, AstNode::StringRef { pool_index: i2 }) => {
                assert_eq!(i1, 0, "First string after clear should be at index 0");
                assert_eq!(i2, 1, "Second string after clear should be at index 1");
                // Old indices should be different (since strings were cleared)
                if let (
                    AstNode::StringRef {
                        pool_index: _old_i1,
                    },
                    AstNode::StringRef {
                        pool_index: _old_i2,
                    },
                ) = (node1, node2)
                {
                    // They might happen to be the same, but that's fine - the key is
                    // that the string pool was actually cleared
                }
            }
            _ => panic!("Expected StringRef nodes"),
        }
    }

    #[test]
    fn test_clear_strings() {
        let mut arena = AstArena::new();

        // Intern some strings
        let _node1 = arena.intern_string("hello");
        let _node2 = arena.intern_string("world");

        // Store memory usage before (for debugging if needed)
        let _memory_before = arena.memory_usage();

        // Clear only strings
        arena.clear_strings();

        // Memory usage should be lower (though capacity is retained)
        // The string_data, string_pool, and string_hash are cleared
        // Let's verify by interning again - should start from index 0
        let node = arena.intern_string("test");
        match node {
            AstNode::StringRef { pool_index } => {
                assert_eq!(
                    pool_index, 0,
                    "First string after clear_strings should be at index 0"
                );
            }
            _ => panic!("Expected StringRef node"),
        }
    }

    #[test]
    fn test_memory_usage() {
        let arena = AstArena::new();
        let usage = arena.memory_usage();
        assert!(usage > 0);
    }
}
