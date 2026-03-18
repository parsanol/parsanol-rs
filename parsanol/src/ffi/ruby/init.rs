//! Ruby module initialization

use magnus::{function, Error, Module, Ruby};

use super::dynamic::{
    register_ruby_callback_with_global_registry, unregister_ruby_callback_from_global_registry,
};
use super::parser::{is_available, parse, parse_batch, parse_with_builder};
use crate::portable::dynamic::{
    clear_dynamic_callbacks, dynamic_callback_count, get_dynamic_callback_description,
    has_dynamic_callback,
};

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
    let module = ruby.define_module("Parsanol")?;
    let native_module = module.define_module("Native")?;

    // =========================================================================
    // PUBLIC API - What most users need
    // =========================================================================

    // Main parsing method - returns clean AST with lazy line/column support
    native_module.define_module_function("parse", function!(parse, 2))?;

    // =========================================================================
    // LOW-LEVEL API - For advanced users / debugging
    // =========================================================================

    // Availability check
    native_module.define_module_function("is_available", function!(is_available, 0))?;

    // Batch parsing (returns flat u64 array - for debugging/benchmarks)
    native_module.define_module_function("parse_batch", function!(parse_batch, 2))?;

    // Streaming builder callback
    native_module.define_module_function("parse_with_builder", function!(parse_with_builder, 3))?;

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

    Ok(())
}
