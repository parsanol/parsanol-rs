//! Ruby FFI support for dynamic callbacks
//!
//! This module provides Ruby-specific implementations for dynamic atom
//! resolution.

use crate::portable::dynamic::{DynamicCallback, DynamicContext};
use crate::portable::grammar::Atom;
use magnus::{value::ReprValue, Error, IntoValue, Module, RClass, Ruby, TryConvert, Value};

/// Ruby dynamic callback wrapper
///
/// This struct wraps a Ruby callback ID and implements the `DynamicCallback`.
/// When `resolve` is called, it invokes Ruby via FFI.
pub struct RubyDynamicCallback {
    /// The callback ID (used to look up the Ruby proc)
    callback_id: u64,
    /// Description for debugging
    description: String,
}

impl RubyDynamicCallback {
    /// Create a new Ruby dynamic callback
    pub fn new(callback_id: u64, description: String) -> Self {
        Self {
            callback_id,
            description,
        }
    }

    /// Get the callback ID
    pub fn id(&self) -> u64 {
        self.callback_id
    }
}

impl DynamicCallback for RubyDynamicCallback {
    fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
        // Get Ruby instance
        let ruby = Ruby::get().ok()?;

        // Build context hash for Ruby
        let ruby_ctx = build_ruby_context(ctx, &ruby)?;

        // Get Parsanol::Native::Dynamic module from Ruby
        let object_class: RClass = ruby.class_object();
        let parsanol_mod: RClass = object_class.const_get::<_, RClass>("Parsanol").ok()?;
        let native_mod: RClass = parsanol_mod.const_get::<_, RClass>("Native").ok()?;
        let dynamic_mod: Value = native_mod.const_get::<_, Value>("Dynamic").ok()?;

        // Call Dynamic.invoke_from_rust(callback_id, context)
        let result: Result<Value, Error> =
            dynamic_mod.funcall("invoke_from_rust", (self.callback_id, ruby_ctx));

        match result {
            Ok(value) => {
                // Check if nil using ReprValue trait
                if value.is_nil() {
                    return None;
                }
                // Convert to Atom
                ruby_value_to_atom(value)
            }
            Err(_) => None,
        }
    }

    fn description(&self) -> &str {
        &self.description
    }
}

/// Build a Ruby context hash from a DynamicContext
fn build_ruby_context(ctx: &DynamicContext, ruby: &Ruby) -> Option<Value> {
    let hash = ruby.hash_new();

    // Add input
    let _ = hash.aset(ruby.to_symbol("input"), ctx.input());

    // Add position
    let _ = hash.aset(ruby.to_symbol("pos"), ctx.pos() as i64);

    // Add remaining
    let _ = hash.aset(ruby.to_symbol("remaining"), ctx.remaining());

    // Add captures as a hash
    let captures_hash = ruby.hash_new();

    for name in ctx.captures.names() {
        if let Some(value) = ctx.captures.get(name) {
            let text = value.get_text(ctx.input());
            let _ = captures_hash.aset(ruby.to_symbol(name.as_str()), text);
        }
    }

    let _ = hash.aset(ruby.to_symbol("captures"), captures_hash);

    // Convert RHash to Value
    Some(hash.into_value_with(ruby))
}

/// Convert a Ruby value to an Atom
fn ruby_value_to_atom(value: Value) -> Option<Atom> {
    // Check if it's a string - TryConvert only takes one argument
    if let Ok(s) = TryConvert::try_convert(value) {
        return Some(Atom::Str { pattern: s });
    }

    // Check if the value responds to to_json (would be a Parsanol atom object)
    // respond_to returns Result<bool, Error>
    let responds_to_json: bool = value.respond_to("to_json", false).ok()?;
    if responds_to_json {
        // Call to_json to get the JSON string
        let json_result: Result<String, Error> = value.funcall("to_json", ());
        if let Ok(json_str) = json_result {
            // Parse JSON to Atom
            if let Ok(atom) = serde_json::from_str::<Atom>(&json_str) {
                return Some(atom);
            }
        }
    }

    None
}

/// Register a Ruby callback with the global dynamic callback registry
pub fn register_ruby_callback_with_global_registry(callback_id: u64, description: String) -> u64 {
    let callback = RubyDynamicCallback::new(callback_id, description);
    crate::portable::dynamic::register_dynamic_callback_with_id(callback_id, Box::new(callback));
    callback_id
}

/// Unregister a Ruby callback from the global registry
pub fn unregister_ruby_callback_from_global_registry(id: u64) -> bool {
    crate::portable::dynamic::unregister_dynamic_callback(id)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ruby_callback_creation() {
        let callback = RubyDynamicCallback::new(1, "test callback".to_string());
        assert_eq!(callback.id(), 1);
        assert_eq!(callback.description(), "test callback");
    }
}
