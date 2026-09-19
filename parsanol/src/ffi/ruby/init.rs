//! Ruby module initialization

use magnus::{function, Error, Module, Ruby};

use super::dynamic::{
    register_ruby_callback_with_global_registry, unregister_ruby_callback_from_global_registry,
};
use super::parser::{
    cacheable_atom_count, clear_grammar_cache, grammar_cache_capacity, grammar_cache_size,
    is_available, optimized_atom_count, parse, parse_batch, parse_fresh, parse_handle,
    parse_handle_events, parse_handle_prefix, parse_with_builder, parse_with_stats,
    register_grammar, release_grammar,
};
use crate::portable::dynamic::{
    clear_dynamic_callbacks, dynamic_callback_count, get_dynamic_callback_description,
    has_dynamic_callback,
};

// CRuby exports this to let extensions declare Ractor safety for the
// bindings they define afterwards. rb-sys does not generate a binding
// for the static-inline helper, so declare it directly; the symbol
// resolves against libruby in every link that compiles this module.
extern "C" {
    fn rb_ext_ractor_safe(flag: bool);
}

// ============================================================================
// FFI wrapper functions for Ruby
// ============================================================================

/// Register a Ruby callback with the global registry
///
/// @param callback_id [Integer] The callback ID (from Ruby registry)
/// @param description [String] Description for debugging
/// @return [Integer] The callback ID
fn ruby_register_callback(callback_id: u64, description: String) -> u64 {
    register_ruby_callback_with_global_registry(callback_id, description)
}

/// Unregister a Ruby callback
///
/// @param id [Integer] The callback ID to unregister
/// @return [Boolean] True if the callback was removed
fn ruby_unregister_callback(id: u64) -> bool {
    unregister_ruby_callback_from_global_registry(id)
}

/// Get the description of a registered callback
///
/// @param id [Integer] The callback ID
/// @return [String, nil] The description or nil if not found
fn ruby_get_callback_description(id: u64) -> Option<String> {
    get_dynamic_callback_description(id)
}

/// Get the number of registered dynamic callbacks
///
/// @return [Integer] Number of callbacks
fn ruby_callback_count() -> usize {
    dynamic_callback_count()
}

/// Clear all registered dynamic callbacks
///
/// @return [nil]
fn ruby_clear_callbacks() {
    clear_dynamic_callbacks()
}

/// Check if a callback is registered
///
/// @param id [Integer] The callback ID
/// @return [Boolean] True if registered
fn ruby_has_callback(id: u64) -> bool {
    has_dynamic_callback(id)
}

/// Initialize the Ruby native extension module
#[magnus::init]
pub fn init(ruby: &Ruby) -> Result<(), Error> {
    // The parse API keeps all global state behind Mutexes/OnceLocks (or
    // thread-locals), so its functions are Ractor-callable: consumers
    // can run native parses on Ractor pools instead of forking. Must be
    // declared before the bindings are defined.
    unsafe {
        rb_ext_ractor_safe(true);
        if std::env::var("PARSANOL_VM_DEBUG").is_ok() {
            eprintln!("parsanol: rb_ext_ractor_safe(true) applied");
        }
    }

    let module = ruby.define_module("Parsanol")?;
    let native_module = module.define_module("Native")?;

    // =========================================================================
    // PUBLIC API - What most users need
    // =========================================================================

    // Main parsing method - returns clean AST with lazy line/column support
    // Named _parse_raw to avoid conflict with Ruby wrapper's parse method
    native_module.define_module_function("_parse_raw", function!(parse, 2))?;

    // Memory-bounded parsing - no cache, fresh arena per call
    native_module.define_module_function("_parse_fresh_raw", function!(parse_fresh, 2))?;

    // Batch parsing method - returns flat u64 array for minimal FFI overhead
    // Named _parse_batch_raw to avoid conflict with Ruby wrapper's parse_batch method
    native_module.define_module_function("_parse_batch_raw", function!(parse_batch, 2))?;

    // Handle-based parsing: register a grammar once, then parse by handle.
    // Avoids the per-call JSON marshal + hash and copies of the input string.
    native_module.define_module_function("_register_grammar", function!(register_grammar, 1))?;
    native_module.define_module_function("_release_grammar", function!(release_grammar, 1))?;
    native_module.define_module_function("_parse_handle", function!(parse_handle, 2))?;
    native_module
        .define_module_function("_parse_handle_prefix", function!(parse_handle_prefix, 2))?;
    native_module
        .define_module_function("_parse_handle_events", function!(parse_handle_events, 2))?;

    // =========================================================================
    // LOW-LEVEL API - For advanced users / debugging
    // =========================================================================

    // Availability check
    native_module.define_module_function("is_available", function!(is_available, 0))?;

    // Streaming builder callback
    native_module.define_module_function("parse_with_builder", function!(parse_with_builder, 3))?;

    // Parsing with cache statistics (for debugging/benchmarks)
    native_module.define_module_function("parse_with_stats", function!(parse_with_stats, 2))?;

    // =========================================================================
    // GRAMMAR CACHE MANAGEMENT - For batch processing and memory management
    // =========================================================================

    // Clear the grammar cache to free memory
    native_module
        .define_module_function("clear_grammar_cache", function!(clear_grammar_cache, 0))?;

    // Get current number of cached grammars
    native_module.define_module_function("grammar_cache_size", function!(grammar_cache_size, 0))?;

    // Get grammar cache capacity
    native_module.define_module_function(
        "grammar_cache_capacity",
        function!(grammar_cache_capacity, 0),
    )?;

    // =========================================================================
    // DYNAMIC CALLBACKS - For advanced use cases
    // =========================================================================

    native_module
        .define_module_function("register_callback", function!(ruby_register_callback, 2))?;
    native_module.define_module_function(
        "unregister_callback",
        function!(ruby_unregister_callback, 1),
    )?;
    native_module.define_module_function(
        "get_callback_description",
        function!(ruby_get_callback_description, 1),
    )?;
    native_module.define_module_function("callback_count", function!(ruby_callback_count, 0))?;
    native_module.define_module_function("clear_callbacks", function!(ruby_clear_callbacks, 0))?;
    native_module.define_module_function("has_callback", function!(ruby_has_callback, 1))?;
    native_module
        .define_module_function("optimized_atom_count", function!(optimized_atom_count, 1))?;
    native_module
        .define_module_function("cacheable_atom_count", function!(cacheable_atom_count, 1))?;

    Ok(())
}
