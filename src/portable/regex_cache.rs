//! Thread-local regex cache for pattern compilation
//!
//! Compiled regex patterns are cached to avoid recompilation overhead.
//! Uses thread-local storage for safe concurrent access without locks.
//!
//! # Cache Statistics
//!
//! The cache tracks hits and misses for performance monitoring:
//!
//! ```
//! use parsanol::portable::regex_cache::{get_or_compile, stats, clear_cache};
//!
//! clear_cache();
//! let _ = get_or_compile("[0-9]+");  // miss (compile)
//! let _ = get_or_compile("[0-9]+");  // hit (cached)
//! let s = stats();
//! assert_eq!(s.hits, 1);
//! assert_eq!(s.misses, 1);
//! ```

use hashbrown::HashMap;
use regex::Regex;
use std::cell::RefCell;

/// Cache statistics for monitoring
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: usize,
    /// Number of cache misses (compilations)
    pub misses: usize,
    /// Number of patterns currently cached
    pub size: usize,
}

thread_local! {
    /// Thread-local cache of compiled regex patterns
    static REGEX_CACHE: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());

    /// Thread-local cache statistics
    static CACHE_STATS: RefCell<CacheStats> = const { RefCell::new(CacheStats { hits: 0, misses: 0, size: 0 }) };
}

/// Get or compile a regex pattern
///
/// This function caches compiled patterns for reuse. Thread-safe via
/// thread-local storage.
///
/// # Arguments
/// * `pattern` - The regex pattern string
///
/// # Returns
/// * `Some(Regex)` if the pattern is valid
/// * `None` if the pattern is invalid
#[inline]
pub fn get_or_compile(pattern: &str) -> Option<Regex> {
    REGEX_CACHE.with(|cache| {
        // Check if already compiled
        if let Some(regex) = cache.borrow().get(pattern) {
            // Cache hit
            CACHE_STATS.with(|stats| {
                stats.borrow_mut().hits += 1;
            });
            return Some(regex.clone());
        }

        // Cache miss - compile and cache
        match Regex::new(pattern) {
            Ok(regex) => {
                cache
                    .borrow_mut()
                    .insert(pattern.to_string(), regex.clone());

                // Update stats
                CACHE_STATS.with(|stats| {
                    let mut s = stats.borrow_mut();
                    s.misses += 1;
                    s.size = cache.borrow().len();
                });

                Some(regex)
            }
            Err(_) => {
                // Still count as a miss even for invalid patterns
                CACHE_STATS.with(|stats| {
                    stats.borrow_mut().misses += 1;
                });
                None
            }
        }
    })
}

/// Clear the regex cache
///
/// Call this to free memory if many unique patterns have been compiled.
pub fn clear_cache() {
    REGEX_CACHE.with(|cache| cache.borrow_mut().clear());
    CACHE_STATS.with(|stats| {
        let mut s = stats.borrow_mut();
        s.hits = 0;
        s.misses = 0;
        s.size = 0;
    });
}

/// Get the number of cached patterns
pub fn cache_size() -> usize {
    REGEX_CACHE.with(|cache| cache.borrow().len())
}

/// Get cache statistics for monitoring
///
/// Returns hit/miss counts and current cache size for the current thread.
pub fn stats() -> CacheStats {
    CACHE_STATS.with(|stats| {
        let mut s = *stats.borrow();
        s.size = cache_size();
        s
    })
}

/// Reset cache statistics (keeps cached patterns)
pub fn reset_stats() {
    CACHE_STATS.with(|stats| {
        let mut s = stats.borrow_mut();
        s.hits = 0;
        s.misses = 0;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_compilation() {
        clear_cache();

        // First access compiles
        let r1 = get_or_compile("[0-9]+");
        assert!(r1.is_some());
        assert_eq!(cache_size(), 1);

        // Second access uses cache
        let r2 = get_or_compile("[0-9]+");
        assert!(r2.is_some());
        assert_eq!(cache_size(), 1);

        // Different pattern adds to cache
        let r3 = get_or_compile("[a-z]+");
        assert!(r3.is_some());
        assert_eq!(cache_size(), 2);
    }

    #[test]
    fn test_invalid_pattern() {
        clear_cache();

        let r = get_or_compile("[invalid");
        assert!(r.is_none());
    }

    #[test]
    fn test_matching() {
        clear_cache();

        let r = get_or_compile("[a-zA-Z_][a-zA-Z0-9_]*").unwrap();
        let text = "hello_world123 rest";

        let m = r.find(text);
        assert!(m.is_some());
        let m = m.unwrap();
        assert_eq!(m.start(), 0);
        assert_eq!(m.end(), 14); // "hello_world123" is 14 characters
        assert_eq!(m.as_str(), "hello_world123");
    }

    #[test]
    fn test_stats() {
        clear_cache();

        // Initial stats should be zero
        let s = stats();
        assert_eq!(s.hits, 0);
        assert_eq!(s.misses, 0);
        assert_eq!(s.size, 0);

        // First access is a miss (compilation)
        let _ = get_or_compile("[0-9]+");
        let s = stats();
        assert_eq!(s.hits, 0);
        assert_eq!(s.misses, 1);
        assert_eq!(s.size, 1);

        // Second access is a hit (cached)
        let _ = get_or_compile("[0-9]+");
        let s = stats();
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 1);
        assert_eq!(s.size, 1);

        // Invalid pattern is also a miss
        let _ = get_or_compile("[invalid");
        let s = stats();
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 2);
    }

    #[test]
    fn test_reset_stats() {
        clear_cache();

        let _ = get_or_compile("[0-9]+");
        let _ = get_or_compile("[0-9]+");

        let s = stats();
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 1);

        reset_stats();

        let s = stats();
        assert_eq!(s.hits, 0);
        assert_eq!(s.misses, 0);
        assert_eq!(s.size, 1); // Size should still be 1
    }
}
