//! Custom Atom Extension Points
//!
//! This module provides a mechanism for extending parsanol with custom parsing
//! logic that can't be expressed with the standard grammar atoms.
//!
//! # Overview
//!
//! The [`CustomAtom`] trait allows you to implement custom parsing logic that
//! can be registered with the parser and used in grammars. Custom atoms are
//! identified by a unique ID and are looked up at parse time.
//!
//! # Example
//!
//! ```
//! use parsanol::portable::custom::{CustomAtom, CustomResult, register_custom_atom};
//!
//! /// A custom atom that matches balanced parentheses
//! struct BalancedParens;
//!
//! impl CustomAtom for BalancedParens {
//!     fn parse(&self, input: &str, pos: usize) -> Option<CustomResult> {
//!         let bytes = input.as_bytes();
//!         if pos >= bytes.len() || bytes[pos] != b'(' {
//!             return None;
//!         }
//!
//!         let mut depth = 1;
//!         let mut current = pos + 1;
//!
//!         while current < bytes.len() && depth > 0 {
//!             match bytes[current] {
//!                 b'(' => depth += 1,
//!                 b')' => depth -= 1,
//!                 _ => {}
//!             }
//!             current += 1;
//!         }
//!
//!         if depth == 0 {
//!             Some(CustomResult {
//!                 end_pos: current,
//!                 value: None, // Return None for simple matchers
//!             })
//!         } else {
//!             None
//!         }
//!     }
//!
//!     fn description(&self) -> &str {
//!         "balanced parentheses"
//!     }
//! }
//!
//! // Register the custom atom
//! let atom_id = 1000; // Use unique IDs (>= 1000 to avoid conflicts)
//! register_custom_atom(atom_id, Box::new(BalancedParens));
//! ```

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Result of a custom atom parse
#[derive(Debug, Clone)]
pub struct CustomResult {
    /// The position after the match (exclusive)
    pub end_pos: usize,
    /// Optional value to return (if None, the matched text is returned)
    pub value: Option<crate::portable::ast::AstNode>,
}

/// Trait for custom parsing logic
///
/// Implement this trait to create custom atoms that can be registered
/// with the parser and used in grammars.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` because they may be called from
/// multiple threads.
pub trait CustomAtom: Send + Sync {
    /// Parse input starting at the given position
    ///
    /// # Arguments
    ///
    /// * `input` - The full input string
    /// * `pos` - The starting position (byte offset)
    ///
    /// # Returns
    ///
    /// * `Some(CustomResult)` if the match succeeds
    /// * `None` if the match fails
    fn parse(&self, input: &str, pos: usize) -> Option<CustomResult>;

    /// Get a description of this custom atom
    ///
    /// Used for error messages and debugging.
    fn description(&self) -> &str;
}

// ============================================================================
// Global Registry
// ============================================================================

/// Global registry for custom atoms
static CUSTOM_REGISTRY: OnceLock<Mutex<CustomAtomRegistry>> = OnceLock::new();

/// Internal registry structure
struct CustomAtomRegistry {
    atoms: HashMap<u64, Box<dyn CustomAtom>>,
    next_id: u64,
}

impl CustomAtomRegistry {
    fn new() -> Self {
        Self {
            atoms: HashMap::new(),
            next_id: 1000, // Start IDs at 1000 to avoid conflicts with built-in atoms
        }
    }
}

/// Get or initialize the global registry
fn get_registry() -> &'static Mutex<CustomAtomRegistry> {
    CUSTOM_REGISTRY.get_or_init(|| Mutex::new(CustomAtomRegistry::new()))
}

/// Register a custom atom with a specific ID
///
/// # Arguments
///
/// * `id` - Unique identifier for the atom (use >= 1000 to avoid conflicts)
/// * `atom` - The custom atom implementation
///
/// # Returns
///
/// The ID that was registered.
///
/// # Panics
///
/// Panics if the ID is already registered.
///
/// # Example
///
/// ```
/// use parsanol::portable::custom::{CustomAtom, CustomResult, register_custom_atom};
///
/// struct MyMatcher;
/// impl CustomAtom for MyMatcher {
///     fn parse(&self, _input: &str, _pos: usize) -> Option<CustomResult> { None }
///     fn description(&self) -> &str { "my matcher" }
/// }
///
/// register_custom_atom(1000, Box::new(MyMatcher));
/// ```
pub fn register_custom_atom(id: u64, atom: Box<dyn CustomAtom>) -> u64 {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap();

    if guard.atoms.contains_key(&id) {
        panic!(
            "Custom atom ID {} is already registered. Use a unique ID.",
            id
        );
    }

    guard.atoms.insert(id, atom);
    id
}

/// Register a custom atom with an auto-generated ID
///
/// # Returns
///
/// The auto-generated ID (>= 1000).
///
/// # Example
///
/// ```
/// use parsanol::portable::custom::{CustomAtom, CustomResult, register_custom_atom_auto};
///
/// struct MyMatcher;
/// impl CustomAtom for MyMatcher {
///     fn parse(&self, _input: &str, _pos: usize) -> Option<CustomResult> { None }
///     fn description(&self) -> &str { "my matcher" }
/// }
///
/// let id = register_custom_atom_auto(Box::new(MyMatcher));
/// assert!(id >= 1000);
/// ```
pub fn register_custom_atom_auto(atom: Box<dyn CustomAtom>) -> u64 {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap();

    let id = guard.next_id;
    guard.next_id += 1;
    guard.atoms.insert(id, atom);
    id
}

/// Unregister a custom atom
///
/// # Returns
///
/// `true` if the atom was found and removed, `false` if it wasn't registered.
///
/// # Example
///
/// ```
/// use parsanol::portable::custom::{CustomAtom, CustomResult, register_custom_atom, unregister_custom_atom};
///
/// struct MyMatcher;
/// impl CustomAtom for MyMatcher {
///     fn parse(&self, _input: &str, _pos: usize) -> Option<CustomResult> { None }
///     fn description(&self) -> &str { "my matcher" }
/// }
///
/// let id = register_custom_atom(2000, Box::new(MyMatcher));
/// assert!(unregister_custom_atom(id));
/// assert!(!unregister_custom_atom(id)); // Already removed
/// ```
pub fn unregister_custom_atom(id: u64) -> bool {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap();
    guard.atoms.remove(&id).is_some()
}

/// Get a custom atom by ID
///
/// # Returns
///
/// `Some(description)` if found, `None` if not registered.
///
/// # Note
///
/// Due to Rust's lifetime system, this returns the description string rather
/// than a reference to the trait object. For parsing, use `parse_custom_atom()`
/// directly.
///
/// # Example
///
/// ```
/// use parsanol::portable::custom::{CustomAtom, CustomResult, register_custom_atom, get_custom_atom_description};
///
/// struct MyMatcher;
/// impl CustomAtom for MyMatcher {
///     fn parse(&self, _input: &str, _pos: usize) -> Option<CustomResult> { None }
///     fn description(&self) -> &str { "my matcher" }
/// }
///
/// let id = register_custom_atom(3000, Box::new(MyMatcher));
/// let desc = get_custom_atom_description(id);
/// assert!(desc.is_some());
/// assert_eq!(desc.unwrap(), "my matcher");
/// ```
pub fn get_custom_atom_description(id: u64) -> Option<String> {
    let registry = get_registry();
    let guard = registry.lock().unwrap();
    guard.atoms.get(&id).map(|b| b.description().to_string())
}

/// Parse using a custom atom by ID
///
/// # Returns
///
/// The result of the custom atom's parse method, or None if not registered.
///
/// # Example
///
/// ```
/// use parsanol::portable::custom::{CustomAtom, CustomResult, register_custom_atom, parse_custom_atom};
///
/// struct MyMatcher;
/// impl CustomAtom for MyMatcher {
///     fn parse(&self, input: &str, pos: usize) -> Option<CustomResult> {
///         if input[pos..].starts_with("foo") {
///             Some(CustomResult { end_pos: pos + 3, value: None })
///         } else {
///             None
///         }
///     }
///     fn description(&self) -> &str { "foo matcher" }
/// }
///
/// let id = register_custom_atom(3500, Box::new(MyMatcher));
/// let result = parse_custom_atom(id, "foobar", 0);
/// assert!(result.is_some());
/// assert_eq!(result.unwrap().end_pos, 3);
/// ```
pub fn parse_custom_atom(id: u64, input: &str, pos: usize) -> Option<CustomResult> {
    let registry = get_registry();
    let guard = registry.lock().unwrap();
    guard.atoms.get(&id).and_then(|atom| atom.parse(input, pos))
}

/// Check if a custom atom is registered
///
/// # Example
///
/// ```
/// use parsanol::portable::custom::{has_custom_atom, register_custom_atom, CustomAtom, CustomResult};
///
/// struct MyMatcher;
/// impl CustomAtom for MyMatcher {
///     fn parse(&self, _input: &str, _pos: usize) -> Option<CustomResult> { None }
///     fn description(&self) -> &str { "my matcher" }
/// }
///
/// assert!(!has_custom_atom(9999));
/// register_custom_atom(9999, Box::new(MyMatcher));
/// assert!(has_custom_atom(9999));
/// ```
pub fn has_custom_atom(id: u64) -> bool {
    let registry = get_registry();
    let guard = registry.lock().unwrap();
    guard.atoms.contains_key(&id)
}

/// Get the number of registered custom atoms
///
/// # Example
///
/// ```
/// use parsanol::portable::custom::{custom_atom_count, register_custom_atom_auto, CustomAtom, CustomResult};
///
/// struct MyMatcher;
/// impl CustomAtom for MyMatcher {
///     fn parse(&self, _input: &str, _pos: usize) -> Option<CustomResult> { None }
///     fn description(&self) -> &str { "my matcher" }
/// }
///
/// let initial_count = custom_atom_count();
/// register_custom_atom_auto(Box::new(MyMatcher));
/// assert_eq!(custom_atom_count(), initial_count + 1);
/// ```
pub fn custom_atom_count() -> usize {
    let registry = get_registry();
    let guard = registry.lock().unwrap();
    guard.atoms.len()
}

/// Clear all registered custom atoms
///
/// # Warning
///
/// This is intended for testing purposes only. Using this in production
/// could cause grammars with custom atoms to fail.
pub fn clear_custom_atoms() {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap();
    guard.atoms.clear();
    guard.next_id = 1000;
}

// ============================================================================
// Built-in Custom Atoms
// ============================================================================

/// A custom atom that matches balanced parentheses
pub struct BalancedParens;

impl CustomAtom for BalancedParens {
    fn parse(&self, input: &str, pos: usize) -> Option<CustomResult> {
        let bytes = input.as_bytes();
        if pos >= bytes.len() || bytes[pos] != b'(' {
            return None;
        }

        let mut depth = 1;
        let mut current = pos + 1;

        while current < bytes.len() && depth > 0 {
            match bytes[current] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            current += 1;
        }

        if depth == 0 {
            Some(CustomResult {
                end_pos: current,
                value: None,
            })
        } else {
            None
        }
    }

    fn description(&self) -> &str {
        "balanced parentheses"
    }
}

/// A custom atom that matches balanced brackets
pub struct BalancedBrackets;

impl CustomAtom for BalancedBrackets {
    fn parse(&self, input: &str, pos: usize) -> Option<CustomResult> {
        let bytes = input.as_bytes();
        if pos >= bytes.len() || bytes[pos] != b'[' {
            return None;
        }

        let mut depth = 1;
        let mut current = pos + 1;

        while current < bytes.len() && depth > 0 {
            match bytes[current] {
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
            current += 1;
        }

        if depth == 0 {
            Some(CustomResult {
                end_pos: current,
                value: None,
            })
        } else {
            None
        }
    }

    fn description(&self) -> &str {
        "balanced brackets"
    }
}

/// A custom atom that matches balanced braces
pub struct BalancedBraces;

impl CustomAtom for BalancedBraces {
    fn parse(&self, input: &str, pos: usize) -> Option<CustomResult> {
        let bytes = input.as_bytes();
        if pos >= bytes.len() || bytes[pos] != b'{' {
            return None;
        }

        let mut depth = 1;
        let mut current = pos + 1;

        while current < bytes.len() && depth > 0 {
            match bytes[current] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            current += 1;
        }

        if depth == 0 {
            Some(CustomResult {
                end_pos: current,
                value: None,
            })
        } else {
            None
        }
    }

    fn description(&self) -> &str {
        "balanced braces"
    }
}

// ============================================================================
// Well-known Custom Atom IDs
// ============================================================================

/// Well-known custom atom IDs
///
/// These IDs are reserved for built-in custom atoms provided by parsanol.
pub mod well_known {
    /// Balanced parentheses: `( ... )`
    pub const BALANCED_PARENS: u64 = 100;

    /// Balanced brackets: `[ ... ]`
    pub const BALANCED_BRACKETS: u64 = 101;

    /// Balanced braces: `{ ... }`
    pub const BALANCED_BRACES: u64 = 102;
}

/// Initialize built-in custom atoms
///
/// This is called automatically when the first custom atom operation is performed.
#[allow(dead_code)]
fn init_builtin_atoms() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        // Use a temporary scope to avoid holding the lock during registration
        let _ = register_custom_atom(well_known::BALANCED_PARENS, Box::new(BalancedParens));
        let _ = register_custom_atom(well_known::BALANCED_BRACKETS, Box::new(BalancedBrackets));
        let _ = register_custom_atom(well_known::BALANCED_BRACES, Box::new(BalancedBraces));
    });
}

// Ensure built-in atoms are initialized when the module is first used
#[allow(dead_code)]
fn ensure_init() {
    init_builtin_atoms();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        struct TestAtom;
        impl CustomAtom for TestAtom {
            fn parse(&self, _input: &str, _pos: usize) -> Option<CustomResult> {
                None
            }
            fn description(&self) -> &str {
                "test"
            }
        }

        let id = register_custom_atom_auto(Box::new(TestAtom));
        assert!(id >= 1000);

        let desc = get_custom_atom_description(id);
        assert!(desc.is_some());
        assert_eq!(desc.unwrap(), "test");

        assert!(unregister_custom_atom(id));
        assert!(!has_custom_atom(id));
    }

    #[test]
    fn test_balanced_parens() {
        let atom = BalancedParens;

        // Valid cases
        assert!(atom.parse("()", 0).is_some());
        assert!(atom.parse("(())", 0).is_some());
        assert!(atom.parse("(a(b)c)", 0).is_some());

        // Check end position
        let result = atom.parse("()", 0).unwrap();
        assert_eq!(result.end_pos, 2);

        let result = atom.parse("(abc)def", 0).unwrap();
        assert_eq!(result.end_pos, 5);

        // Invalid cases
        assert!(atom.parse("", 0).is_none());
        assert!(atom.parse("(", 0).is_none());
        assert!(atom.parse(")(", 0).is_none());
        assert!(atom.parse("((", 0).is_none());
    }

    #[test]
    fn test_balanced_brackets() {
        let atom = BalancedBrackets;

        assert!(atom.parse("[]", 0).is_some());
        assert!(atom.parse("[[]]", 0).is_some());
        assert!(atom.parse("[a[b]c]", 0).is_some());

        let result = atom.parse("[abc]def", 0).unwrap();
        assert_eq!(result.end_pos, 5);
    }

    #[test]
    fn test_balanced_braces() {
        let atom = BalancedBraces;

        assert!(atom.parse("{}", 0).is_some());
        assert!(atom.parse("{{}}", 0).is_some());
        assert!(atom.parse("{a{b}c}", 0).is_some());

        let result = atom.parse("{abc}def", 0).unwrap();
        assert_eq!(result.end_pos, 5);
    }
}
