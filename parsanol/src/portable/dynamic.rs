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
use std::sync::{Arc, Mutex, OnceLock};

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
    /// Current capture state (may be empty if no captures). Capture
    /// atoms store their parsed subtrees here (see
    /// `CaptureState::node_values`), so dynamic blocks read the TREE
    /// (capture parity), and the subtrees travel with the state.
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
    pub fn get_capture_text<'s>(&'s self, name: &str) -> Option<std::borrow::Cow<'s, str>> {
        self.captures.get(name).map(|v| v.get_text(self.input()))
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

    /// Resolve a self-contained fragment grammar at runtime.
    ///
    /// Use when the resolved atoms reference each other by index and
    /// therefore only make sense inside their own grammar (the shape
    /// host-language bridges produce when a callback returns an atom
    /// subtree). Takes precedence over #resolve when it returns Some.
    fn resolve_fragment(&self, _ctx: &DynamicContext) -> Option<(super::grammar::Grammar, usize)> {
        None
    }
}

// ============================================================================
// Global Registry
// ============================================================================

/// Global registry for dynamic callbacks
static DYNAMIC_REGISTRY: OnceLock<Mutex<DynamicRegistry>> = OnceLock::new();

/// Internal registry structure
struct DynamicRegistry {
    callbacks: hashbrown::HashMap<u64, Arc<dyn DynamicCallback>>,
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
    register_dynamic_callback_inner(next_free_id(), Arc::from(callback))
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
    register_dynamic_callback_inner(id, Arc::from(callback))
}

fn next_free_id() -> u64 {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap_or_else(|e| e.into_inner());
    let id = guard.next_id;
    guard.next_id += 1;
    id
}

fn register_dynamic_callback_inner(id: u64, callback: Arc<dyn DynamicCallback>) -> u64 {
    let registry = get_registry();
    let mut guard = registry.lock().unwrap_or_else(|e| e.into_inner());
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
    with_dynamic_callback(id, |cb| cb.resolve(ctx))
}

/// Maximum dynamic nesting depth (mutually recursive fragments).
pub const MAX_DYNAMIC_DEPTH: u32 = 256;
/// Maximum dynamic invocations per top-level parse (backtracking
/// re-invokes callbacks on every retry, so a shallow-but-divergent
/// dispatch still needs a total budget).
pub const MAX_DYNAMIC_CALLS: u32 = 10_000;

thread_local! {
    static DYNAMIC_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static DYNAMIC_CALLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// RAII token for one dynamic-invocation level; dropping it leaves
/// the level and, at the outermost level, resets the call budget.
pub(crate) struct DynamicGuard;

impl Drop for DynamicGuard {
    fn drop(&mut self) {
        DYNAMIC_DEPTH.with(|d| {
            let next = d.get().saturating_sub(1);
            d.set(next);
            if next == 0 {
                DYNAMIC_CALLS.with(|c| c.set(0));
            }
        });
    }
}

/// Enter one dynamic-invocation level, enforcing the recursion and
/// budget guards (GH-76): a grammar whose dispatch never converges
/// must fail loudly, not hang or balloon memory. `None` means a
/// guard tripped — treat it as a parse failure at the call site.
pub(crate) fn enter_dynamic() -> Option<DynamicGuard> {
    let depth_ok = DYNAMIC_DEPTH.with(|d| {
        let v = d.get();
        if v < MAX_DYNAMIC_DEPTH {
            d.set(v + 1);
            true
        } else {
            false
        }
    });
    if !depth_ok {
        return None;
    }
    let budget_ok = DYNAMIC_CALLS.with(|c| {
        let v = c.get();
        if v < MAX_DYNAMIC_CALLS {
            c.set(v + 1);
            true
        } else {
            false
        }
    });
    if budget_ok {
        Some(DynamicGuard)
    } else {
        DYNAMIC_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        None
    }
}

// ============================================================================
// Capture-write channel (parsanol-ruby#80)
// ============================================================================

thread_local! {
    static CAPTURE_WRITES: std::cell::RefCell<Vec<(String, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Record capture writes made by a host callback block (the bridge
/// reads back the block's mutated context hash and posts the diff
/// here). Engines drain the writes into their capture state right
/// after the callback resolves — inside whatever capture scope
/// encloses the dynamic atom, so backtracking discards them exactly
/// like capture-atom writes when the enclosing branch fails.
pub fn note_capture_writes(writes: Vec<(String, String)>) {
    if writes.is_empty() {
        return;
    }
    CAPTURE_WRITES.with(|w| w.borrow_mut().extend(writes));
}

/// Drain pending capture writes into a capture state, converting each
/// to a literal-text value. Called by both engines after callback
/// resolution.
pub fn drain_capture_writes_into(captures: &mut CaptureState) {
    CAPTURE_WRITES.with(|w| {
        let mut pending = w.borrow_mut();
        for (name, text) in pending.drain(..) {
            captures.store(&name, super::capture_state::CaptureValue::text(text));
        }
    });
}

// ============================================================================
// Dispatch cache (parsanol-ruby#80, item 2)
// ============================================================================

/// A resolved fragment, cached for reuse when the same callback is
/// invoked again at the same position with the same captures (and the
/// same input). Blocks see exactly (input, pos, captures), so a
/// deterministic block's fragment — and its capture writes — are a
/// pure function of that key.
pub struct CachedFragment {
    /// The resolved fragment grammar.
    pub grammar: std::sync::Arc<super::grammar::Grammar>,
    /// The fragment's root atom.
    pub root: usize,
    /// Capture writes the block performed; replayed on cache hits so
    /// side effects match the uncached path exactly.
    pub writes: Vec<(String, String)>,
}

thread_local! {
    static DISPATCH_CACHE: std::cell::RefCell<
        std::collections::HashMap<(u64, usize, u64), CachedFragment>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
    static INPUT_HASH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

const DISPATCH_CACHE_CAP: usize = 512;

/// Begin a parse: fixes the input identity for the dispatch cache
/// (blocks can dispatch on any byte of the input, so its hash is part
/// of every key) and clears both the cache and pending capture
/// writes.
pub fn begin_parse(input: &str) {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in input.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    INPUT_HASH.with(|c| c.set(h));
    DISPATCH_CACHE.with(|c| c.borrow_mut().clear());
    clear_capture_writes();
}

fn capture_signature(captures: &CaptureState, input: &str) -> u64 {
    let mut h: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut names: Vec<&String> = captures.names().collect();
    names.sort();
    for name in names {
        if let Some(value) = captures.get(name) {
            h ^= name.len() as u64;
            h = h.wrapping_mul(0x1000_0000_01B3);
            for b in name.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x1000_0000_01B3);
            }
            let text = value.get_text(input);
            h ^= text.len() as u64;
            h = h.wrapping_mul(0x1000_0000_01B3);
            for b in text.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x1000_0000_01B3);
            }
        }
    }
    // Capture subtrees participate in the signature: a block that
    // reads a tree (capture semantics) can resolve differently for
    // the same texts but different subtrees.
    let mut node_sigs: Vec<(String, u64)> = captures
        .node_fingerprints()
        .map(|(name, fp)| (name.clone(), fp))
        .collect();
    node_sigs.sort();
    for (name, fp) in node_sigs {
        h ^= name.len() as u64;
        h = h.wrapping_mul(0x1000_0000_01B3);
        for b in name.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x1000_0000_01B3);
        }
        h ^= fp;
        h = h.wrapping_mul(0x1000_0000_01B3);
    }
    h
}

/// A dispatch-cache hit: the fragment grammar, its root atom, and
/// the capture writes to replay.
pub type FragmentHit = (
    std::sync::Arc<super::grammar::Grammar>,
    usize,
    Vec<(String, String)>,
);

/// Look up a cached fragment for this (callback, position, captures,
/// input). Hit => the engine replays the grammar and the recorded
/// capture writes without re-entering the host.
pub fn cached_fragment(
    callback_id: u64,
    pos: usize,
    captures: &CaptureState,
    input: &str,
) -> Option<FragmentHit> {
    let input_hash = INPUT_HASH.with(|c| c.get());
    let key = (
        callback_id,
        pos,
        capture_signature(captures, input) ^ input_hash,
    );
    DISPATCH_CACHE.with(|c| {
        c.borrow()
            .get(&key)
            .map(|f| (std::sync::Arc::clone(&f.grammar), f.root, f.writes.clone()))
    })
}

/// Store a resolved fragment for the key. `writes` are the pending
/// capture writes the block produced; they are drained and replayed
/// together with the fragment on later hits.
pub fn store_dispatch_fragment(
    callback_id: u64,
    pos: usize,
    captures: &CaptureState,
    input: &str,
    grammar: super::grammar::Grammar,
    root: usize,
    writes: Vec<(String, String)>,
) -> std::sync::Arc<super::grammar::Grammar> {
    let input_hash = INPUT_HASH.with(|c| c.get());
    let key = (
        callback_id,
        pos,
        capture_signature(captures, input) ^ input_hash,
    );
    let arc = std::sync::Arc::new(grammar);
    DISPATCH_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= DISPATCH_CACHE_CAP {
            cache.clear();
        }
        cache.insert(
            key,
            CachedFragment {
                grammar: std::sync::Arc::clone(&arc),
                root,
                writes,
            },
        );
    });
    arc
}

/// Snapshot the pending capture writes (used by the engine to record
/// what a freshly resolved block wrote, for replay on cache hits).
pub fn take_pending_writes() -> Vec<(String, String)> {
    CAPTURE_WRITES.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

/// Whether a dynamic dispatch is currently on the stack: nested
/// fragment parsers are NOT parse entry points and must not reset
/// per-parse state (dispatch cache, pending writes, input hash).
pub fn in_dynamic_dispatch() -> bool {
    DYNAMIC_DEPTH.with(|d| d.get() > 0)
}

/// Clear any pending writes (parse entry points call this so writes
/// can never leak across parses).
pub fn clear_capture_writes() {
    CAPTURE_WRITES.with(|w| w.borrow_mut().clear());
}

/// Run a closure with the registered callback. Engines use this to
/// access the full callback surface (resolve / resolve_fragment)
/// without cloning.
pub fn with_dynamic_callback<T>(
    id: u64,
    f: impl FnOnce(&dyn DynamicCallback) -> Option<T>,
) -> Option<T> {
    // Clone the callback handle and DROP the registry lock before
    // invoking: callbacks re-enter the engine (fragment parses hit
    // further Dynamic atoms), and lock-across-invoke deadlocked at
    // zero CPU (GH-76 follow-up).
    let cb = {
        let registry = get_registry();
        let guard = registry.lock().unwrap_or_else(|e| e.into_inner());
        guard.callbacks.get(&id).cloned()?
    };
    f(cb.as_ref())
}

/// Get the description of a registered callback by ID.
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
            if capture_text.as_ref() == value.as_str() {
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
        assert_eq!(ctx.get_capture_text("name").as_deref(), Some("hello"));
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
