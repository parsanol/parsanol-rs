//! Dynamic callback support for runtime atom resolution
//!
//! This module provides a generic mechanism for dynamic atom resolution,
//! where the atom to parse is determined at runtime based on the current
//! parsing context (input, position, and captures).
//!
//! # Architecture
//!
//! The dynamic callback system follows these principles:
//! - **Generic FFI**: Not tied to any specific language runtime
//! - **Context-Aware**: Callbacks receive capture state for context-sensitive parsing
//! - **Registry-Based**: Global registry for callback lookup by ID
//!
//! # Example (Rust Native)
//!
//! ```
//! use parsanol::portable::dynamic::{DynamicCallback, DynamicContext, register_dynamic_callback};
//! use parsanol::portable::grammar::Atom;
//!
//! struct KeywordResolver;
//!
//! impl DynamicCallback for KeywordResolver {
//!     fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
//!         // Return different atoms based on captures
//!         Some(Atom::Str { pattern: "keyword".to_string() })
//!     }
//!
//!     fn description(&self) -> &str {
//!         "keyword resolver"
//!     }
//! }
//!
//! let id = register_dynamic_callback(Box::new(KeywordResolver));
//! ```

use super::capture_state::CaptureState;
use super::grammar::Atom;
use std::sync::{Mutex, OnceLock};

// ============================================================================
// Dynamic Context
// ============================================================================

/// Context provided to dynamic callbacks
///
/// This struct provides read-only access to the parsing context,
/// including the input string, current position, and captured values.
#[derive(Debug, Clone)]
pub struct DynamicContext {
    /// The input string being parsed
    pub input: String,
    /// Current byte position in the input
    pub pos: usize,
    /// Current capture state (may be empty if no captures)
    pub captures: CaptureState,
}

impl DynamicContext {
    /// Create a new dynamic context
    #[inline]
    pub fn new(input: &str, pos: usize, captures: CaptureState) -> Self {
        Self {
            input: input.to_string(),
            pos,
            captures,
        }
    }

    /// Get the input string
    #[inline]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Get the current position
    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Get a captured value by name
    #[inline]
    pub fn get_capture(&self, name: &str) -> Option<super::capture_state::CaptureValue> {
        self.captures.get(name)
    }

    /// Get the text of a captured value
    #[inline]
    pub fn get_capture_text(&self, name: &str) -> Option<&str> {
        self.captures.get(name).map(|v| v.get_text(&self.input))
    }

    /// Check if a capture exists
    #[inline]
    pub fn has_capture(&self, name: &str) -> bool {
        self.captures.contains(name)
    }

    /// Get the remaining input from the current position
    #[inline]
    pub fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    /// Check if at end of input
    #[inline]
    pub fn is_at_end(&self) -> bool {
        self.pos >= self.input.len()
    }
}

// ============================================================================
// Dynamic Callback Trait
// ============================================================================

/// Trait for dynamic atom resolution
///
/// Implementations receive the current parsing context and return
/// an `Atom` to parse, or `None` to fail the parse.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` because they may be called from
/// multiple threads.
///
/// # Performance
///
/// Callbacks are called during parsing, so they should be fast.
/// Avoid expensive operations like heap allocations when possible.
///
/// # FFI Implementations
///
/// For FFI implementations (Ruby, Python, etc.), implement this trait
/// using the FFI's callback mechanism:
///
/// ```rust,ignore
/// // Example for Ruby FFI
/// struct RubyCallback {
///     proc: magnus::Value,
/// }
///
/// impl DynamicCallback for RubyCallback {
///     fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
///         // Call Ruby proc via FFI
///         // ...
///     }
/// }
/// ```
pub trait DynamicCallback: Send + Sync {
    /// Resolve the atom to parse based on context
    ///
    /// # Arguments
    ///
    /// * `ctx` - The current parsing context (read-only)
    ///
    /// # Returns
    ///
    /// * `Some(atom)` - The atom to parse at the current position
    /// * `None` - Fail the parse (like a predicate that returned false)
    fn resolve(&self, ctx: &DynamicContext) -> Option<Atom>;

    /// Get a description of this callback
    ///
    /// Used for error messages and debugging.
    fn description(&self) -> &str;
}

// ============================================================================
// Global Registry
// ============================================================================

/// Global registry for dynamic callbacks
static DYNAMIC_REGISTRY: OnceLock<Mutex<DynamicRegistry>> = OnceLock::new();

/// Internal registry structure
struct DynamicRegistry {
    callbacks: hashbrown::HashMap<u64, Box<dyn DynamicCallback>>,
    next_id: u64,
}

impl DynamicRegistry {
    fn new() -> Self {
        Self {
            callbacks: hashbrown::HashMap::new(),
            next_id: 1, // Start at 1 (0 is reserved for "no callback")
        }
    }
}

/// Get or initialize the global registry
fn get_registry() -> &'static Mutex<DynamicRegistry> {
    DYNAMIC_REGISTRY.get_or_init(|| Mutex::new(DynamicRegistry::new()))
}

/// Register a dynamic callback
///
/// # Returns
///
/// A unique ID for the callback, which can be used in `Atom::Dynamic`.
///
/// # Example
///
/// ```
/// use parsanol::portable::dynamic::{DynamicCallback, DynamicContext, register_dynamic_callback};
/// use parsanol::portable::grammar::Atom;
///
/// struct MyResolver;
/// impl DynamicCallback for MyResolver {
///     fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
///         Some(Atom::Str { pattern: "test".to_string() })
///     }
///     fn description(&self) -> &str { "my resolver" }
/// }
///
/// let id = register_dynamic_callback(Box::new(MyResolver));
/// assert!(id > 0);
/// ```
pub fn register_dynamic_callback(callback: Box<dyn DynamicCallback>) -> u64 {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap();

    let id = guard.next_id;
    guard.next_id += 1;
    guard.callbacks.insert(id, callback);
    id
}

/// Register a dynamic callback with a specific ID
///
/// # Arguments
///
/// * `id` - Unique identifier for the callback
/// * `callback` - The callback implementation
///
/// # Returns
///
/// The ID that was registered.
///
/// # Panics
///
/// Panics if the ID is already registered.
pub fn register_dynamic_callback_with_id(id: u64, callback: Box<dyn DynamicCallback>) -> u64 {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap();

    if guard.callbacks.contains_key(&id) {
        panic!(
            "Dynamic callback ID {} is already registered. Use a unique ID.",
            id
        );
    }

    guard.callbacks.insert(id, callback);
    id
}

/// Unregister a dynamic callback
///
/// # Returns
///
/// `true` if the callback was found and removed, `false` if not registered.
pub fn unregister_dynamic_callback(id: u64) -> bool {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap();
    guard.callbacks.remove(&id).is_some()
}

/// Invoke a dynamic callback
///
/// # Returns
///
/// The result of the callback, or `None` if not registered.
pub fn invoke_dynamic_callback(id: u64, ctx: &DynamicContext) -> Option<Atom> {
    let registry = get_registry();
    let guard = registry.lock().unwrap();
    guard.callbacks.get(&id).and_then(|cb| cb.resolve(ctx))
}

/// Get a callback's description
///
/// # Returns
///
/// The description string, or `None` if not registered.
pub fn get_dynamic_callback_description(id: u64) -> Option<String> {
    let registry = get_registry();
    let guard = registry.lock().unwrap();
    guard
        .callbacks
        .get(&id)
        .map(|cb| cb.description().to_string())
}

/// Check if a callback is registered
pub fn has_dynamic_callback(id: u64) -> bool {
    let registry = get_registry();
    let guard = registry.lock().unwrap();
    guard.callbacks.contains_key(&id)
}

/// Get the number of registered callbacks
pub fn dynamic_callback_count() -> usize {
    let registry = get_registry();
    let guard = registry.lock().unwrap();
    guard.callbacks.len()
}

/// Clear all registered callbacks
///
/// # Warning
///
/// This is intended for testing purposes only.
pub fn clear_dynamic_callbacks() {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap();
    guard.callbacks.clear();
    guard.next_id = 1;
}

// ============================================================================
// Built-in Callbacks
// ============================================================================

/// A callback that always returns a fixed atom
pub struct ConstCallback {
    atom: Atom,
    description: String,
}

impl ConstCallback {
    /// Create a new const callback
    pub fn new(atom: Atom, description: &str) -> Self {
        Self {
            atom,
            description: description.to_string(),
        }
    }
}

impl DynamicCallback for ConstCallback {
    fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
        Some(self.atom.clone())
    }

    fn description(&self) -> &str {
        &self.description
    }
}

/// A callback that checks a capture and returns different atoms
pub struct CaptureSwitchCallback {
    capture_name: String,
    cases: Vec<(String, Atom)>,
    default: Option<Atom>,
    description: String,
}

impl CaptureSwitchCallback {
    /// Create a new switch callback
    ///
    /// # Arguments
    ///
    /// * `capture_name` - Name of the capture to check
    /// * `cases` - List of (value, atom) pairs
    /// * `default` - Default atom if no case matches (None = fail)
    pub fn new(capture_name: &str, cases: Vec<(String, Atom)>, default: Option<Atom>) -> Self {
        Self {
            capture_name: capture_name.to_string(),
            cases,
            default,
            description: format!("switch on {}", capture_name),
        }
    }
}

impl DynamicCallback for CaptureSwitchCallback {
    fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
        let capture_text = ctx.get_capture_text(&self.capture_name)?;

        for (value, atom) in &self.cases {
            if capture_text == value {
                return Some(atom.clone());
            }
        }

        self.default.clone()
    }

    fn description(&self) -> &str {
        &self.description
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::capture_state::CaptureValue;

    #[test]
    fn test_dynamic_context() {
        let mut captures = CaptureState::new();
        captures.store("name", CaptureValue::new(0, 5));

        let ctx = DynamicContext::new("hello world", 5, captures);

        assert_eq!(ctx.pos(), 5);
        assert_eq!(ctx.input(), "hello world");
        assert_eq!(ctx.remaining(), " world");
        assert!(!ctx.is_at_end());

        assert!(ctx.has_capture("name"));
        assert_eq!(ctx.get_capture_text("name"), Some("hello"));
    }

    #[test]
    fn test_register_and_invoke() {
        struct TestCallback;
        impl DynamicCallback for TestCallback {
            fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
                if ctx.remaining().starts_with("foo") {
                    Some(Atom::Str {
                        pattern: "foo".to_string(),
                    })
                } else {
                    None
                }
            }
            fn description(&self) -> &str {
                "test callback"
            }
        }

        let id = register_dynamic_callback(Box::new(TestCallback));
        assert!(id > 0);
        assert!(has_dynamic_callback(id));

        let ctx = DynamicContext::new("foobar", 0, CaptureState::new());
        let result = invoke_dynamic_callback(id, &ctx);
        assert!(result.is_some());

        let ctx2 = DynamicContext::new("bazbar", 0, CaptureState::new());
        let result2 = invoke_dynamic_callback(id, &ctx2);
        assert!(result2.is_none());

        assert!(unregister_dynamic_callback(id));
        assert!(!has_dynamic_callback(id));
    }

    #[test]
    fn test_const_callback() {
        let callback = ConstCallback::new(
            Atom::Str {
                pattern: "test".to_string(),
            },
            "const test",
        );

        let ctx = DynamicContext::new("anything", 0, CaptureState::new());
        let result = callback.resolve(&ctx);
        assert!(result.is_some());

        match result.unwrap() {
            Atom::Str { pattern } => assert_eq!(pattern, "test"),
            _ => panic!("Expected Str atom"),
        }
    }

    #[test]
    fn test_switch_callback() {
        let callback = CaptureSwitchCallback::new(
            "type",
            vec![
                (
                    "int".to_string(),
                    Atom::Str {
                        pattern: "integer".to_string(),
                    },
                ),
                (
                    "str".to_string(),
                    Atom::Str {
                        pattern: "string".to_string(),
                    },
                ),
            ],
            Some(Atom::Str {
                pattern: "unknown".to_string(),
            }),
        );

        // Test matching "int"
        let mut captures1 = CaptureState::new();
        captures1.store("type", CaptureValue::new(0, 3));
        let ctx1 = DynamicContext::new("int", 0, captures1);
        let result1 = callback.resolve(&ctx1);
        assert!(result1.is_some());

        // Test matching "str"
        let mut captures2 = CaptureState::new();
        captures2.store("type", CaptureValue::new(0, 3));
        let ctx2 = DynamicContext::new("str", 0, captures2);
        let result2 = callback.resolve(&ctx2);
        assert!(result2.is_some());

        // Test no capture
        let ctx3 = DynamicContext::new("anything", 0, CaptureState::new());
        let result3 = callback.resolve(&ctx3);
        assert!(result3.is_none());
    }

    #[test]
    fn test_context_at_end() {
        let ctx = DynamicContext::new("short", 5, CaptureState::new());
        assert!(ctx.is_at_end());
        assert_eq!(ctx.remaining(), "");
    }
}
