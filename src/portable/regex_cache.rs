//! Thread-local regex cache for pattern compilation
//!
//! Compiled regex patterns are cached to avoid recompilation overhead.
//! Uses thread-local storage for safe concurrent access.

use hashbrown::HashMap;
use regex::Regex;
use std::cell::RefCell;

thread_local! {
    /// Thread-local cache of compiled regex patterns
    static REGEX_CACHE: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());
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
            return Some(regex.clone());
        }

        // Compile and cache
        match Regex::new(pattern) {
            Ok(regex) => {
                cache
                    .borrow_mut()
                    .insert(pattern.to_string(), regex.clone());
                Some(regex)
            }
            Err(_) => None,
        }
    })
}

/// Clear the regex cache
///
/// Call this to free memory if many unique patterns have been compiled.
pub fn clear_cache() {
    REGEX_CACHE.with(|cache| cache.borrow_mut().clear());
}

/// Get the number of cached patterns
pub fn cache_size() -> usize {
    REGEX_CACHE.with(|cache| cache.borrow().len())
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
}
