//! Parser functions for Ruby FFI
//!
//! # Public API
//!
//! ```ruby
//! result = Parsanol::Native.parse(grammar_json, input)
//! ```
//!
//! Slice objects support lazy line/column computation:
//!
//! ```ruby
//! slice = result[:name]
//! slice.offset            # => 42 (always available)
//! slice.content           # => "hello" (always available)
//! slice.line_and_column   # => [5, 1] (computed lazily on first call)
//! ```

use crate::ffi::shared::flatten_ast_to_u64;
use crate::portable::{AstArena, Grammar, PortableParser};
use magnus::{Error, Ruby, Value};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use super::builder::RubyBuilder;
use super::transform::transform_ast;

// Thread-safe global grammar cache
static GRAMMAR_CACHE: std::sync::OnceLock<Mutex<hashbrown::HashMap<u64, Grammar>>> =
    std::sync::OnceLock::new();

fn get_grammar_cache() -> &'static Mutex<hashbrown::HashMap<u64, Grammar>> {
    GRAMMAR_CACHE.get_or_init(|| Mutex::new(hashbrown::HashMap::new()))
}

fn hash_string(s: &str) -> u64 {
    let mut hasher = ahash::AHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Check if native extension is available
pub fn is_available() -> bool {
    true
}

// ============================================================================
// PUBLIC API - What most users need
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
pub fn parse(grammar_json: String, input: String) -> Result<Value, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe with caching)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = serde_json::from_str(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = cache.lock().unwrap();
            guard.insert(hash, grammar.clone());
            grammar
        }
    };

    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // Transform AST to Ruby format with full sequence/repetition handling
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

    // Get or compile grammar (thread-safe)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = serde_json::from_str(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = cache.lock().unwrap();
            guard.insert(hash, grammar.clone());
            grammar
        }
    };

    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // Get cache statistics before parser is consumed
    let (cache, _) = parser.into_cache();
    let (hits, misses, hit_rate) = cache.stats();

    // Transform AST to Ruby format
    let result = transform_ast(&ast, &arena, &input, &ruby)?;

    Ok((result, hits, misses, hit_rate))
}

/// Parse using batch FFI - returns flat array instead of Ruby objects
///
/// This is a low-level function primarily used for debugging and benchmarks.
/// Most users should use `parse()` instead.
pub fn parse_batch(grammar_json: String, input: String) -> Result<Vec<u64>, Error> {
    let ruby = Ruby::get().unwrap();

    // Get or compile grammar (thread-safe)
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = serde_json::from_str(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = cache.lock().unwrap();
            guard.insert(hash, grammar.clone());
            grammar
        }
    };

    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);

    let ast = parser
        .parse()
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;

    // Flatten AST to u64 array using unified implementation
    let mut result = Vec::new();
    flatten_ast_to_u64(&ast, &arena, &input, &mut result);
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

    // Get or compile grammar
    let hash = hash_string(&grammar_json);
    let grammar = {
        let cache = get_grammar_cache();
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&hash) {
            cached.clone()
        } else {
            drop(guard);
            let grammar: Grammar = serde_json::from_str(&grammar_json)
                .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
            let mut guard = cache.lock().unwrap();
            guard.insert(hash, grammar.clone());
            grammar
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
