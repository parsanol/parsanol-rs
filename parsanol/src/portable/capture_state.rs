//! Capture state management for named captures
//!
//! This module provides a high-performance capture state implementation
//! that supports named captures with generational scope management.
//!
//! # Architecture
//!
//! The capture state uses a **generational scope model**:
//! - Each scope push records the current capture count
//! - Each scope pop removes only captures added in that scope
//! - This provides O(1) push_scope and O(c_scope) pop_scope
//!
//! # Performance Characteristics
//!
//! | Operation | Complexity |
//! |-----------|------------|
//! | `store` | O(1) amortized |
//! | `get` | O(1) |
//! | `push_scope` | O(1) |
//! | `pop_scope` | O(c_scope) |
//!
//! # Zero-Copy Design
//!
//! `CaptureValue` stores only offset and length, not the actual text.
//! Text is retrieved by slicing the input string at access time.

use ahash::AHashMap;
use std::cell::RefCell;

/// Maximum allowed scope depth to prevent stack overflow
pub const MAX_SCOPE_DEPTH: usize = 1000;

/// A zero-copy capture value
///
/// Stores only the offset and length of captured text.
/// The actual text is retrieved by slicing the input string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureValue {
    /// Byte offset into the input string
    pub offset: usize,
    /// Length in bytes
    pub length: usize,
}

impl CaptureValue {
    /// Create a new capture value
    #[inline]
    pub fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }

    /// Get the captured text from the input string
    ///
    /// # Safety
    ///
    /// The caller must ensure that `offset + length <= input.len()`.
    #[inline]
    pub fn get_text<'a>(&self, input: &'a str) -> &'a str {
        &input[self.offset..self.offset + self.length]
    }

    /// Get the end position (offset + length)
    #[inline]
    pub fn end(&self) -> usize {
        self.offset + self.length
    }

    /// Check if this capture is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

/// Snapshot of capture state for backtracking
///
/// Used to restore capture state when a parse alternative fails.
#[derive(Debug, Clone)]
pub struct CaptureSnapshot {
    /// Number of captures when snapshot was taken
    capture_count: usize,
    /// Scope stack depth when snapshot was taken
    scope_depth: usize,
}

/// Entry in the capture order list
///
/// Tracks whether a capture is new or shadowing an existing one.
#[derive(Debug, Clone)]
enum CaptureEntry {
    /// New capture added in current scope
    New(String),
    /// Shadow of existing capture, with the old value
    Shadow(String, CaptureValue),
}

/// Capture state with generational scope management
///
/// This struct manages named captures with support for:
/// - Nested scopes (for lookahead isolation, etc.)
/// - Shadowing (inner scope captures can shadow outer ones)
/// - Backtracking (via snapshots)
/// - Zero-copy text storage
///
/// # Thread Safety
///
/// This type is NOT thread-safe. It should be owned by a single parser
/// instance. Use `Clone` to create snapshots for backtracking.
///
/// # Example
///
/// ```
/// use parsanol::portable::capture_state::{CaptureState, CaptureValue};
///
/// let mut state = CaptureState::new();
///
/// // Store a capture
/// state.store("name", CaptureValue::new(0, 5));
/// assert_eq!(state.get("name").unwrap().length, 5);
///
/// // Push a scope (isolates inner captures)
/// state.push_scope();
/// state.store("inner", CaptureValue::new(5, 3));
///
/// // Pop scope discards inner captures
/// state.pop_scope();
/// assert!(state.get("inner").is_none());
/// assert!(state.get("name").is_some()); // outer still visible
/// ```
#[derive(Debug, Clone)]
pub struct CaptureState {
    /// Named captures (name -> value)
    captures: AHashMap<String, CaptureValue>,
    /// Ordered list of capture entries (for efficient scope pop with shadowing)
    capture_order: Vec<CaptureEntry>,
    /// Scope stack - each entry is the capture count at scope entry
    scope_stack: Vec<usize>,
    /// Current scope depth (for overflow protection)
    depth: usize,
}

impl Default for CaptureState {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureState {
    /// Create a new capture state with default capacity
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(16)
    }

    /// Create a new capture state with specified initial capacity
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            captures: AHashMap::with_capacity(capacity),
            capture_order: Vec::with_capacity(capacity),
            scope_stack: Vec::with_capacity(16),
            depth: 0,
        }
    }

    /// Store a named capture
    ///
    /// If a capture with the same name exists, it is shadowed (not overwritten).
    /// The original value will be restored when the current scope is popped.
    ///
    /// # Returns
    ///
    /// `true` if this was a new capture, `false` if it shadowed an existing one.
    #[inline]
    pub fn store(&mut self, name: &str, value: CaptureValue) -> bool {
        if let Some(&old_value) = self.captures.get(name) {
            // Shadowing an existing capture
            self.captures.insert(name.to_string(), value);
            self.capture_order
                .push(CaptureEntry::Shadow(name.to_string(), old_value));
            false
        } else {
            // New capture
            self.captures.insert(name.to_string(), value);
            self.capture_order.push(CaptureEntry::New(name.to_string()));
            true
        }
    }

    /// Get a capture by name
    #[inline]
    pub fn get(&self, name: &str) -> Option<CaptureValue> {
        self.captures.get(name).copied()
    }

    /// Check if a capture exists
    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.captures.contains_key(name)
    }

    /// Get the number of captures
    #[inline]
    pub fn len(&self) -> usize {
        self.captures.len()
    }

    /// Check if there are no captures
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.captures.is_empty()
    }

    /// Get the current scope depth
    #[inline]
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Push a new scope
    ///
    /// Captures made after this call will be discarded when `pop_scope` is called.
    ///
    /// # Panics
    ///
    /// Panics if scope depth exceeds `MAX_SCOPE_DEPTH`.
    #[inline]
    pub fn push_scope(&mut self) {
        if self.depth >= MAX_SCOPE_DEPTH {
            panic!(
                "Scope depth {} exceeds maximum {}",
                self.depth, MAX_SCOPE_DEPTH
            );
        }
        self.scope_stack.push(self.capture_order.len());
        self.depth += 1;
    }

    /// Pop a scope, discarding captures made in that scope
    ///
    /// For shadowed captures, the original value is restored.
    ///
    /// # Returns
    ///
    /// The number of captures that were modified in this scope.
    ///
    /// # Panics
    ///
    /// Panics if there is no scope to pop.
    pub fn pop_scope(&mut self) -> usize {
        let start_len = self.scope_stack.pop().expect("No scope to pop");
        self.depth = self.depth.saturating_sub(1);

        let removed_count = self.capture_order.len() - start_len;

        // Remove/restore captures added in this scope
        for _ in 0..removed_count {
            if let Some(entry) = self.capture_order.pop() {
                match entry {
                    CaptureEntry::New(name) => {
                        self.captures.remove(&name);
                    }
                    CaptureEntry::Shadow(name, old_value) => {
                        // Restore the shadowed value
                        self.captures.insert(name, old_value);
                    }
                }
            }
        }

        removed_count
    }

    /// Create a snapshot for backtracking
    ///
    /// The snapshot records the current state and can be restored
    /// via `restore_snapshot`.
    #[inline]
    pub fn snapshot(&self) -> CaptureSnapshot {
        CaptureSnapshot {
            capture_count: self.capture_order.len(),
            scope_depth: self.scope_stack.len(),
        }
    }

    /// Restore to a previous snapshot
    ///
    /// This discards all captures and scopes added after the snapshot was taken.
    /// Shadowed captures are restored to their original values.
    pub fn restore(&mut self, snapshot: &CaptureSnapshot) {
        // Pop scopes until we're at the right depth
        while self.scope_stack.len() > snapshot.scope_depth {
            self.pop_scope();
        }

        // Remove any captures added after the snapshot (at root level)
        while self.capture_order.len() > snapshot.capture_count {
            if let Some(entry) = self.capture_order.pop() {
                match entry {
                    CaptureEntry::New(name) => {
                        self.captures.remove(&name);
                    }
                    CaptureEntry::Shadow(name, old_value) => {
                        self.captures.insert(name, old_value);
                    }
                }
            }
        }

        self.depth = self.scope_stack.len();
    }

    /// Clear all captures and scopes
    #[inline]
    pub fn clear(&mut self) {
        self.captures.clear();
        self.capture_order.clear();
        self.scope_stack.clear();
        self.depth = 0;
    }

    /// Iterate over all capture names (in order of first occurrence)
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.capture_order.iter().map(|entry| match entry {
            CaptureEntry::New(name) => name,
            CaptureEntry::Shadow(name, _) => name,
        })
    }

    /// Iterate over all captures
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CaptureValue)> {
        self.captures.iter()
    }

    /// Get all captures as a vector of (name, value) pairs in insertion order
    pub fn to_vec(&self) -> Vec<(String, CaptureValue)> {
        self.capture_order
            .iter()
            .filter_map(|entry| {
                let name = match entry {
                    CaptureEntry::New(name) => name,
                    CaptureEntry::Shadow(name, _) => name,
                };
                self.captures.get(name).map(|&value| (name.clone(), value))
            })
            .collect()
    }

    /// Merge captures from another state
    ///
    /// This is useful for combining results from parallel parsing.
    /// Captures from `other` will overwrite existing captures with the same name.
    pub fn merge(&mut self, other: &CaptureState) {
        for (name, &value) in other.iter() {
            if let Some(&old_value) = self.captures.get(name) {
                // Shadowing existing capture
                self.captures.insert(name.clone(), value);
                self.capture_order
                    .push(CaptureEntry::Shadow(name.clone(), old_value));
            } else {
                // New capture
                self.captures.insert(name.clone(), value);
                self.capture_order.push(CaptureEntry::New(name.clone()));
            }
        }
    }
}

// ============================================================================
// Thread-Local Capture State (for dynamic callbacks)
// ============================================================================

thread_local! {
    /// Thread-local capture state for dynamic callbacks
    ///
    /// This allows dynamic callbacks to access the current capture state
    /// without needing to pass it through the call stack.
    static CURRENT_CAPTURES: RefCell<Option<CaptureState>> = const { RefCell::new(None) };
}

/// Set the current capture state (for dynamic callbacks)
///
/// Returns a guard that restores the previous state when dropped.
pub fn set_current_captures(state: CaptureState) -> CaptureGuard {
    let previous = CURRENT_CAPTURES.with(|c| c.replace(Some(state)));
    CaptureGuard { previous }
}

/// Get a reference to the current capture state
///
/// Returns `None` if no capture state is set.
pub fn current_captures() -> Option<CaptureState> {
    CURRENT_CAPTURES.with(|c| c.borrow().clone())
}

/// Get a specific capture from the current state
pub fn current_capture(name: &str) -> Option<CaptureValue> {
    CURRENT_CAPTURES.with(|c| c.borrow().as_ref().and_then(|state| state.get(name)))
}

/// Guard that restores previous capture state when dropped
pub struct CaptureGuard {
    previous: Option<CaptureState>,
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CURRENT_CAPTURES.with(|c| {
            *c.borrow_mut() = self.previous.take();
        });
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_value() {
        let value = CaptureValue::new(7, 5); // "World" in "Hello, World!"
        assert_eq!(value.offset, 7);
        assert_eq!(value.length, 5);
        assert_eq!(value.end(), 12);
        assert!(!value.is_empty());

        let empty = CaptureValue::new(10, 0);
        assert!(empty.is_empty());

        let input = "Hello, World!";
        assert_eq!(value.get_text(input), "World");
    }

    #[test]
    fn test_capture_state_basic() {
        let mut state = CaptureState::new();

        assert!(state.is_empty());
        assert_eq!(state.len(), 0);
        assert_eq!(state.depth(), 0);

        // Store a capture
        assert!(state.store("name", CaptureValue::new(0, 5)));
        assert!(!state.is_empty());
        assert_eq!(state.len(), 1);

        // Overwrite capture
        assert!(!state.store("name", CaptureValue::new(5, 3)));
        assert_eq!(state.len(), 1);

        // Get capture
        let value = state.get("name").unwrap();
        assert_eq!(value.offset, 5);
        assert_eq!(value.length, 3);

        // Non-existent capture
        assert!(state.get("missing").is_none());
        assert!(!state.contains("missing"));
    }

    #[test]
    fn test_scope_push_pop() {
        let mut state = CaptureState::new();

        // Outer capture
        state.store("outer", CaptureValue::new(0, 5));
        assert_eq!(state.len(), 1);

        // Push scope
        state.push_scope();
        assert_eq!(state.depth(), 1);

        // Inner capture
        state.store("inner", CaptureValue::new(5, 3));
        assert_eq!(state.len(), 2);
        assert!(state.contains("inner"));
        assert!(state.contains("outer")); // Outer still visible

        // Pop scope
        let removed = state.pop_scope();
        assert_eq!(removed, 1);
        assert_eq!(state.depth(), 0);
        assert!(!state.contains("inner")); // Inner discarded
        assert!(state.contains("outer")); // Outer still there
    }

    #[test]
    fn test_nested_scopes() {
        let mut state = CaptureState::new();

        state.store("level0", CaptureValue::new(0, 1));
        assert_eq!(state.depth(), 0);

        state.push_scope();
        state.store("level1", CaptureValue::new(1, 1));
        assert_eq!(state.depth(), 1);

        state.push_scope();
        state.store("level2", CaptureValue::new(2, 1));
        assert_eq!(state.depth(), 2);

        // All captures visible
        assert!(state.contains("level0"));
        assert!(state.contains("level1"));
        assert!(state.contains("level2"));

        // Pop to level 1
        let removed = state.pop_scope();
        assert_eq!(removed, 1);
        assert_eq!(state.depth(), 1);
        assert!(!state.contains("level2"));
        assert!(state.contains("level1"));
        assert!(state.contains("level0"));

        // Pop to level 0
        let removed = state.pop_scope();
        assert_eq!(removed, 1);
        assert_eq!(state.depth(), 0);
        assert!(!state.contains("level1"));
        assert!(state.contains("level0"));
    }

    #[test]
    fn test_shadowing() {
        let mut state = CaptureState::new();

        // Outer "x"
        state.store("x", CaptureValue::new(0, 5));
        assert_eq!(state.get("x").unwrap().offset, 0);

        state.push_scope();

        // Shadow "x"
        state.store("x", CaptureValue::new(10, 3));
        assert_eq!(state.get("x").unwrap().offset, 10);

        state.pop_scope();

        // Original "x" restored
        assert_eq!(state.get("x").unwrap().offset, 0);
    }

    #[test]
    fn test_snapshot_restore() {
        let mut state = CaptureState::new();

        state.store("a", CaptureValue::new(0, 1));
        let snapshot = state.snapshot();

        state.store("b", CaptureValue::new(1, 1));
        state.push_scope();
        state.store("c", CaptureValue::new(2, 1));

        assert_eq!(state.len(), 3);
        assert_eq!(state.depth(), 1);

        // Restore to snapshot
        state.restore(&snapshot);

        assert_eq!(state.len(), 1);
        assert_eq!(state.depth(), 0);
        assert!(state.contains("a"));
        assert!(!state.contains("b"));
        assert!(!state.contains("c"));
    }

    #[test]
    fn test_snapshot_with_nested_scopes() {
        let mut state = CaptureState::new();

        state.store("root", CaptureValue::new(0, 1));
        state.push_scope();

        let snapshot = state.snapshot();

        state.store("inner", CaptureValue::new(1, 1));
        state.push_scope();
        state.store("deep", CaptureValue::new(2, 1));

        assert_eq!(state.depth(), 2);

        state.restore(&snapshot);

        assert_eq!(state.depth(), 1);
        assert!(state.contains("root"));
        assert!(!state.contains("inner"));
        assert!(!state.contains("deep"));
    }

    #[test]
    fn test_clear() {
        let mut state = CaptureState::new();

        state.store("a", CaptureValue::new(0, 1));
        state.push_scope();
        state.store("b", CaptureValue::new(1, 1));

        state.clear();

        assert!(state.is_empty());
        assert_eq!(state.depth(), 0);
    }

    #[test]
    fn test_iter() {
        let mut state = CaptureState::new();

        state.store("a", CaptureValue::new(0, 1));
        state.store("b", CaptureValue::new(1, 1));
        state.store("c", CaptureValue::new(2, 1));

        let names: Vec<_> = state.names().cloned().collect();
        assert_eq!(names, vec!["a", "b", "c"]);

        let pairs: Vec<_> = state.to_vec();
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0].0, "a");
        assert_eq!(pairs[1].0, "b");
        assert_eq!(pairs[2].0, "c");
    }

    #[test]
    fn test_merge() {
        let mut state1 = CaptureState::new();
        state1.store("a", CaptureValue::new(0, 1));
        state1.store("b", CaptureValue::new(1, 1));

        let mut state2 = CaptureState::new();
        state2.store("b", CaptureValue::new(10, 2)); // Overwrite
        state2.store("c", CaptureValue::new(2, 1));

        state1.merge(&state2);

        assert_eq!(state1.len(), 3);
        assert_eq!(state1.get("b").unwrap().offset, 10); // Overwritten
        assert!(state1.contains("c"));
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn test_max_scope_depth() {
        let mut state = CaptureState::new();

        for _ in 0..MAX_SCOPE_DEPTH {
            state.push_scope();
        }

        // This should panic
        state.push_scope();
    }

    #[test]
    fn test_thread_local_captures() {
        let mut state = CaptureState::new();
        state.store("test", CaptureValue::new(0, 5));

        {
            let _guard = set_current_captures(state);

            // Can access via current_capture
            let value = current_capture("test");
            assert!(value.is_some());
            assert_eq!(value.unwrap().length, 5);

            // Can clone the whole state
            let cloned = current_captures();
            assert!(cloned.is_some());
        }

        // After guard dropped, no captures
        assert!(current_capture("test").is_none());
    }
}
