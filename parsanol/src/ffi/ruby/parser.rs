//! Parser functions for Ruby FFI
//!
//! # Public API
//!
//! ## High-Level API (Recommended)
//!
//! ```ruby
//! result = Parsanol::Native.parse(grammar, input)
//! # Returns Parslet-compatible AST with lazy line/column support
//! ```
//!
//! ## Raw API (For Custom Transformation)
//!
//! ```ruby
//! result = Parsanol::Native.parse_raw(grammar, input)
//! # Returns raw intermediate format (no transformation)
//! ```
//!
//! # Architecture
//!
//! The parsing pipeline consists of:
//! 1. **Rust parsing** - Fast parsing with packrat memoization
//! 2. **Rust transformation** - `to_parslet_compatible` produces Parslet-compatible AST
//! 3. **Batch encoding** - Flat u64 array for efficient FFI transfer
//! 4. **Ruby decoding** - BatchDecoder produces Ruby Hash/Array/Slice objects
//!
//! # Slice Objects
//!
//! Slice objects support lazy line/column computation:
//!
//! ```ruby
//! slice = result[:name]
//! slice.offset            # => 42 (always available)
//! slice.content           # => "hello" (always available)
//! slice.line_and_column   # => [5, 1] (computed lazily on first call)
//! ```
//!
//! # Batch Format
//!
//! The batch format uses tagged u64 values for efficient FFI:
//!
//! | Tag | Value | Description |
//! |-----|-------|-------------|
//! | 0x00 | - | nil |
//! | 0x01 | 0 or 1 | bool |
//! | 0x02 | value | int |
//! | 0x03 | IEEE bits | float |
//! | 0x04 | offset, length | Slice reference |
//! | 0x05-0x06 | ... | array |
//! | 0x07-0x08 | ... | hash |
//! | 0x09 | len, data... | hash key |
//! | 0x0A | len, data... | inline string |
//! | 0x0B | len, data... | symbol |
//! | 0x0C | items... | repetition marker |
//! | 0x0D | items... | sequence marker |

use crate::ffi::ruby::cache::LruCache;
use crate::ffi::shared::flatten_ast_to_u64;
use crate::portable::{
    to_parslet_compatible, AstArena, Atom, DenseCache, Grammar, ParseError, PortableParser,
};
use magnus::{value::ReprValue, Error, RString, Ruby, Value};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::builder::RubyBuilder;
use super::transform::transform_ast;

/// Default maximum number of grammars to cache.
/// This prevents unbounded memory growth during batch processing.
const DEFAULT_GRAMMAR_CACHE_SIZE: usize = 100;

type GrammarCache = LruCache<u64, Grammar>;

/// Thread-safe global grammar cache with bounded LRU eviction
static GRAMMAR_CACHE: std::sync::OnceLock<Mutex<GrammarCache>> = std::sync::OnceLock::new();

/// Grammar registered by explicit handle, avoiding the per-call JSON
/// marshal + hash of the LRU path. `has_dynamic` gates the zero-copy
/// input borrow: a Dynamic atom calls back into Ruby during the parse,
/// and we must not hold a borrowed `&str` across that.
#[derive(Clone)]
struct HandleEntry {
    grammar: Arc<Grammar>,
    has_dynamic: bool,
}

static HANDLE_MAP: std::sync::OnceLock<Mutex<HashMap<u64, HandleEntry>>> =
    std::sync::OnceLock::new();
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn get_handle_map() -> &'static Mutex<HashMap<u64, HandleEntry>> {
    HANDLE_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_grammar_cache() -> &'static Mutex<GrammarCache> {
    GRAMMAR_CACHE.get_or_init(|| Mutex::new(GrammarCache::new(DEFAULT_GRAMMAR_CACHE_SIZE)))
}

/// Clear the grammar cache, freeing all cached grammars.
///
/// This is useful for batch processing scenarios where you want to
/// limit memory usage by clearing unused grammars.
///
/// # Example
///
/// ```ruby
/// Parsanol::Native.clear_grammar_cache
/// ```
pub fn clear_grammar_cache() {
    if let Some(cache) = GRAMMAR_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            guard.clear();
        }
    }
    if let Some(map) = HANDLE_MAP.get() {
        if let Ok(mut guard) = map.lock() {
            guard.clear();
        }
    }
}

/// Get the current number of cached grammars.
pub fn grammar_cache_size() -> usize {
    if let Some(cache) = GRAMMAR_CACHE.get() {
        if let Ok(guard) = cache.lock() {
            return guard.len();
        }
    }
    0
}

/// Get the grammar cache capacity.
pub fn grammar_cache_capacity() -> usize {
    if let Some(cache) = GRAMMAR_CACHE.get() {
        if let Ok(guard) = cache.lock() {
            return guard.capacity();
        }
    }
    DEFAULT_GRAMMAR_CACHE_SIZE
}

fn hash_string(s: &str) -> u64 {
    let mut hasher = ahash::AHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Deserialize grammar from JSON, applying optimizations.
/// The grammar is cached by hash after optimization.
fn load_grammar(grammar_json: &str) -> Result<Grammar, serde_json::Error> {
    Grammar::from_json(grammar_json)
}

/// Check if native extension is available
pub fn is_available() -> bool {
    true
}

// ============================================================================
// Handle-based API
//
// The JSON-string API pays a marshal + hash of the grammar JSON on every
// call. Registering once and passing a small integer handle removes that
// per-call cost entirely, and lets `parse_handle` borrow the input string
// instead of copying it.
// ============================================================================

/// Register a grammar JSON and return a handle for `parse_handle`.
///
/// # Example
///
/// ```ruby
/// handle = Parsanol::Native._register_grammar(grammar_json)
/// result = Parsanol::Native._parse_handle(handle, input)
/// ```
pub fn register_grammar(grammar_json: String) -> Result<u64, Error> {
    let ruby = Ruby::get().unwrap();
    let grammar: Grammar = load_grammar(&grammar_json)
        .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
    let has_dynamic = grammar
        .atoms
        .iter()
        .any(|a| matches!(a, Atom::Dynamic { .. }));
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    get_handle_map().lock().unwrap().insert(
        handle,
        HandleEntry {
            grammar: Arc::new(grammar),
            has_dynamic,
        },
    );
    Ok(handle)
}

/// Release a grammar previously registered with `register_grammar`.
/// Returns true when a handle was removed.
pub fn release_grammar(handle: u64) -> bool {
    get_handle_map().lock().unwrap().remove(&handle).is_some()
}

/// Parse with a registered grammar handle, borrowing the input string
/// (zero copy) when the grammar has no Dynamic atoms.
pub fn parse_handle(handle: u64, input: RString) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();
    let entry = get_handle_map().lock().unwrap().get(&handle).cloned();
    let Some(entry) = entry else {
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!("unknown grammar handle: {}", handle),
        ));
    };

    // SAFETY: the borrowed &str is used for the duration of this call only.
    // Nothing mutates the input string while we hold the reference: Ruby
    // code only re-enters via Dynamic atoms (excluded by `has_dynamic`);
    // object allocation during transform can run the GC but cannot move or
    // mutate a string (compaction requires an explicit GC.compact that
    // user code cannot run inside this call).
    let input_str: &str = unsafe { input.as_str()? };

    if entry.has_dynamic {
        let owned = input_str.to_string();
        parse_with_grammar(&ruby, &entry.grammar, &owned)
    } else {
        parse_with_grammar(&ruby, &entry.grammar, input_str)
    }
}

/// Parse with a registered grammar handle WITHOUT requiring full-input
/// consumption (the Ruby engine's `prefix: true` mode). Returns
/// [value, end_pos]; unmatched trailing input is simply left over.
pub fn parse_handle_prefix(handle: u64, input: RString) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();
    let entry = get_handle_map().lock().unwrap().get(&handle).cloned();
    let Some(entry) = entry else {
        return Err(Error::new(
            ruby.exception_arg_error(),
            format!("unknown grammar handle: {}", handle),
        ));
    };

    // SAFETY: same borrow discipline as parse_handle.
    let input_str: &str = unsafe { input.as_str()? };

    let mut arena = AstArena::for_input(input_str.len());
    arena.set_input(input_str.to_string());
    let mut parser = PortableParser::new(&entry.grammar, input_str, &mut arena);
    let result = match parser.parse_with_end_pos() {
        Ok(result) => result,
        Err(e) => {
            let diagnostics = parser.failure_diagnostics();
            return Err(Error::new(
                ruby.exception_runtime_error(),
                native_failure_message(&e, diagnostics),
            ));
        }
    };

    let collapsed = crate::ffi::shared::collapse_ast(&result.value, &mut arena);
    let value = transform_ast(&collapsed, &arena, input_str, &ruby)?;
    let pair = ruby.ary_new_capa(2);
    pair.push(value)?;
    pair.push(result.end_pos as i64)?;
    Ok(pair.as_value())
}

/// Shared parse + collapse + transform over an already-resolved grammar.

/// Format a native parse failure for the Ruby tier: a parslet-style
/// expected-set message plus a machine-readable position marker the
/// Ruby side strips before raising Parsanol::ParseFailed.
fn native_failure_message(e: &ParseError, diagnostics: Option<(usize, Vec<String>)>) -> String {
    if let ParseError::Failed { position } = e {
        let (position, expected) = match diagnostics {
            Some((pos, labels)) => (pos, labels),
            None => (*position, Vec::new()),
        };
        let what = if expected.is_empty() {
            "no further input".to_string()
        } else {
            format!("one of [{}]", expected.join(", "))
        };
        // Position suffix comes from Cause#to_s on the Ruby side; the
        // marker line carries the byte offset for cause construction.
        return format!(
            "Failed to match: expected {}\n@@parsanol_pos:{}",
            what, position
        );
    }
    e.to_string()
}

fn parse_with_grammar(ruby: &Ruby, grammar: &Grammar, input: &str) -> Result<Value, Error> {
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(grammar, input, &mut arena);

    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => {
            let diagnostics = parser.failure_diagnostics();
            return Err(Error::new(
                ruby.exception_runtime_error(),
                native_failure_message(&e, diagnostics),
            ));
        }
    };

    // Collapse adjacent input refs in the arena first: the join happens with
    // zero Ruby object churn, so the flat encoding stays small.
    let collapsed = crate::ffi::shared::collapse_ast(&ast, &mut arena);

    // Build Ruby objects directly. A batch-encoding detour (u64 cells
    // boxed into Integers, then re-walked by the Ruby decoder) measured
    // 1.7x SLOWER than parslet on a real ~90-rule compat grammar
    // (issue parsanol-ruby#25) versus ~3x faster with this direct
    // build; the batch path stays in the C-ABI tier where it is the
    // only option. The transformer heuristics on both paths are kept
    // in agreement by the shared differential corpus.
    transform_ast(&collapsed, &arena, input, ruby)
}

// ============================================================================
// HIGH-LEVEL API
// ============================================================================

/// Parse input and return transformed AST with lazy line/column support
///
/// This is the MAIN parsing method that all users should use.
/// It returns a clean AST matching Ruby parser output:
/// - Symbol keys instead of string keys
/// - Merged sequences (unnamed strings discarded when named captures present)
/// - Proper repetition handling (arrays of named captures, joined strings)
/// - Slice objects with lazy line/column computation
///
/// # Performance
///
/// Provides up to 26x speedup over pure Ruby parsing.
/// Line/column is computed lazily only when Slice#line_and_column is called.
///
/// # Arguments
///
/// * `grammar_json` - JSON string containing the grammar definition
/// * `input` - Input string to parse
///
/// # Returns
///
/// Ruby Hash/Array with transformed AST structure. String values are
/// Parsanol::Slice objects that support lazy line/column computation.
///
/// # Example
///
/// ```ruby
/// grammar_json = Parsanol::Native.serialize_grammar(my_atom)
/// result = Parsanol::Native.parse(grammar_json, "hello")
/// # => {name: "hello"@0}
///
/// # Line/column computed lazily on demand
/// slice = result[:name]
/// slice.line_and_column  # => [1, 1]
/// ```
/// Parse with a grammar JSON string (LRU-cached by content hash).
pub fn parse(grammar_json: String, input: String) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            // Re-check in case another thread added it while we were parsing
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    parse_with_grammar(&ruby, &grammar, &input)
}

/// Parse without packrat caching for memory-bounded operation
///
/// This creates a fresh arena and an empty cache (no memoization).
/// Memory usage is bounded by AST size rather than input × atoms.
/// Use for large files where memory is more important than speed.
///
/// Returns the same format as `parse()`.
pub fn parse_fresh(grammar_json: String, input: String) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    // Create fresh arena (no reuse, no memory accumulation)
    let mut arena = AstArena::for_input(input.len());
    // Create empty cache — every lookup misses, no memoization
    let cache = DenseCache::new(0);
    let mut parser = PortableParser::new_with_cache(&grammar, &input, &mut arena, cache);

    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // Transform AST to Ruby format
    transform_ast(&ast, &arena, &input, &ruby)
}

// ============================================================================
// LOW-LEVEL API - For advanced users / debugging
// ============================================================================

/// Parse with cache statistics - returns [ast, cache_hits, cache_misses, hit_rate]
///
/// This is a low-level function for performance debugging.
/// Most users should use `parse()` instead.
pub fn parse_with_stats(
    grammar_json: String,
    input: String,
) -> Result<(Value, u64, u64, f64), Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            // Re-check in case another thread added it while we were parsing
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // Get cache statistics before parser is consumed
    let cache = parser.into_cache();
    let (hits, misses, hit_rate) = cache.stats();

    // Transform AST to Ruby format
    let result = transform_ast(&ast, &arena, &input, &ruby)?;

    Ok((result, hits, misses, hit_rate))
}

/// Parse using batch FFI - returns flat array WITH transformation
///
/// This is the RECOMMENDED batch parsing function. It transforms the AST
/// to Parslet-compatible format BEFORE flattening, so Ruby can decode
/// directly without additional transformation.
///
/// Returns a flat u64 array where the AST is already transformed:
/// - Sequences merged (unnamed discarded when named captures present)
/// - Repetitions properly handled (arrays of named captures)
/// - Consecutive slices joined
pub fn parse_batch(grammar_json: String, input: String) -> Result<Vec<u64>, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            // Re-check in case another thread added it while we were parsing
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    let mut arena = AstArena::for_input(input.len());
    arena.set_input(input.clone());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    // 1. Parse
    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // 2. Transform to Parslet-compatible format
    // This is REQUIRED for Expressir use case - without it, the AST is in
    // raw intermediate format that Builder cannot process.
    let transformed = to_parslet_compatible(&ast, &mut arena, &input);

    // 3. Flatten transformed AST to u64 array
    let mut result = Vec::new();
    flatten_ast_to_u64(&transformed, &arena, &input, &mut result);
    Ok(result)
}

/// Parse with a Ruby builder callback
///
/// This is an advanced function for streaming parsing.
/// Most users should use `parse()` instead.
pub fn parse_with_builder(
    grammar_json: String,
    input: String,
    builder: Value,
) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with LRU caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let mut guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = load_grammar(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = get_grammar_cache().lock().unwrap();
            // Re-check in case another thread added it while we were parsing
            if let Some(cached) = guard.get(&hash) {
                cached.clone()
            } else {
                guard.insert(hash, grammar.clone());
                grammar
            }
        }
    };

    // Create Ruby builder wrapper
    let mut ruby_builder = RubyBuilder::new(builder);

    // Parse with builder
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    parser
        .parse_with_builder(&mut ruby_builder)
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))
}

/// Return optimized atom count for a grammar JSON (diagnostic)
pub fn optimized_atom_count(grammar_json: String) -> Result<usize, Error> {
    let ruby = Ruby::get().unwrap();
    let grammar: Grammar = load_grammar(&grammar_json)
        .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
    Ok(grammar.atom_count())
}

/// Return count of atoms that need packrat caching (diagnostic)
pub fn cacheable_atom_count(grammar_json: String) -> Result<usize, Error> {
    let ruby = Ruby::get().unwrap();
    let grammar: Grammar = load_grammar(&grammar_json)
        .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
    Ok(grammar.cacheable_atom_count())
}
