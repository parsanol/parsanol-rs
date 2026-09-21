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
        let value = self.invoke_block(ctx)?;
        // A plain String is an index-free literal match.
        let pattern: Result<String, Error> = TryConvert::try_convert(value);
        pattern.ok().map(|pattern| Atom::Str { pattern })
    }

    fn resolve_fragment(&self, ctx: &DynamicContext) -> Option<(crate::portable::Grammar, usize)> {
        let value = self.invoke_block(ctx)?;
        let responds: bool = value.respond_to("to_atom_json", false).ok()?;
        if !responds {
            return None;
        }
        let json: Result<String, Error> = value.funcall("to_atom_json", ());
        let grammar = crate::portable::Grammar::from_json(&json.ok()?).ok()?;
        let root = grammar.root;
        Some((grammar, root))
    }

    fn description(&self) -> &str {
        &self.description
    }
}

impl RubyDynamicCallback {
    /// Shared block invocation: builds the Ruby context hash and calls
    /// Parsanol::Native::Dynamic.invoke_from_rust.
    fn invoke_block(&self, ctx: &DynamicContext) -> Option<Value> {
        let trace = std::env::var("PARSANOL_DYN_TRACE").is_ok();
        let Ok(ruby) = Ruby::get() else {
            if trace {
                eprintln!("DYN-BRIDGE: no ruby");
            }
            return None;
        };
        let Some(ruby_ctx) = build_ruby_context(ctx, &ruby) else {
            if trace {
                eprintln!("DYN-BRIDGE: context build failed");
            }
            return None;
        };

        let object_class: RClass = ruby.class_object();
        let parsanol_mod = match object_class.const_get::<_, magnus::RModule>("Parsanol") {
            Ok(m) => m,
            Err(e) => {
                if trace {
                    eprintln!("DYN-BRIDGE: Parsanol const_get failed: {e:?}");
                }
                return None;
            }
        };
        let native_mod = match parsanol_mod.const_get::<_, magnus::RModule>("Native") {
            Ok(m) => m,
            Err(e) => {
                if trace {
                    eprintln!("DYN-BRIDGE: Native const_get failed: {e:?}");
                }
                return None;
            }
        };
        let dynamic_mod = match native_mod.const_get::<_, Value>("Dynamic") {
            Ok(m) => m,
            Err(e) => {
                if trace {
                    eprintln!("DYN-BRIDGE: Dynamic const_get failed: {e:?}");
                }
                return None;
            }
        };

        // The seeded captures travel inside ruby_ctx; remember them so
        // the post-call readback can diff out the block's writes.
        let seeded: Vec<(String, String)> = ctx
            .captures
            .names()
            .filter_map(|n| {
                ctx.captures
                    .get(n)
                    .map(|v| (n.clone(), v.get_text(ctx.input()).into_owned()))
            })
            .collect();

        let result: Result<Value, Error> =
            dynamic_mod.funcall("invoke_from_rust", (self.callback_id, ruby_ctx));
        match result {
            Ok(value) if !value.is_nil() => {
                if trace {
                    eprintln!("DYN-BRIDGE: invoked ok");
                }
                // Contract (GH-80): [atom, post_call_captures]. Unwrap
                // before resolving; the bare-atom shape stays valid.
                let atom = if value.is_kind_of(ruby.class_array()) {
                    let len: usize = value
                        .funcall("size", ())
                        .unwrap_or(0);
                    if len == 2 {
                        value.funcall("first", ()).unwrap_or(value)
                    } else {
                        value
                    }
                } else {
                    value
                };
                self.readback_capture_writes(&ruby, &value, &seeded);
                Some(atom)
            }
            Ok(_) => {
                if trace {
                    eprintln!("DYN-BRIDGE: invoked -> nil");
                }
                None
            }
            Err(e) => {
                if trace {
                    eprintln!("DYN-BRIDGE: funcall failed: {e:?}");
                }
                None
            }
        }
    }
}

impl RubyDynamicCallback {
    /// Read back the captures hash the block may have mutated
    /// (parsanol-ruby#80) and post the diff to the engine's write
    /// channel. Contract: `invoke_from_rust` returns
    /// [result, captures_after]; the bare-value shape is tolerated
    /// with no writeback.
    fn readback_capture_writes(&self, ruby: &Ruby, value: &Value, seeded: &[(String, String)]) {
        let pair: Result<(Value, Value), Error> = TryConvert::try_convert(*value);
        let Ok((_, caps_after)) = pair else {
            return;
        };
        let Ok(hash): Result<magnus::RHash, Error> = TryConvert::try_convert(caps_after) else {
            return;
        };
        let mut writes: Vec<(String, String)> = Vec::new();
        let Ok(pairs): Result<magnus::RArray, Error> = hash.funcall("to_a", ()) else {
            return;
        };
        for entry in pairs.into_iter() {
            let Ok(pair): Result<magnus::RArray, Error> = TryConvert::try_convert(entry) else {
                continue;
            };
            let mut it = pair.into_iter();
            let (k, v) = match (it.next(), it.next()) {
                (Some(k), Some(v)) => (k, v),
                _ => continue,
            };
            let Ok(name): Result<String, Error> = TryConvert::try_convert(k) else {
                continue;
            };
            let Ok(text): Result<String, Error> = TryConvert::try_convert(v) else {
                continue;
            };
            let unchanged = seeded.iter().any(|(sn, st)| *sn == name && *st == text);
            if !unchanged {
                writes.push((name, text));
            }
        }
        if !writes.is_empty() {
            if std::env::var("PARSANOL_DYN_TRACE").is_ok() {
                eprintln!("DYN-BRIDGE: capture writes {writes:?}");
            }
            crate::portable::dynamic::note_capture_writes(writes);
        }
        let _ = ruby;
    }
}

/// Build a Ruby context hash from a DynamicContext
fn build_ruby_context(ctx: &DynamicContext, ruby: &Ruby) -> Option<Value> {
    let hash = ruby.hash_new();
    let _ = hash.aset(ruby.to_symbol("input"), ctx.input());
    let _ = hash.aset(ruby.to_symbol("pos"), ctx.pos() as i64);
    let _ = hash.aset(ruby.to_symbol("remaining"), ctx.remaining());
    let captures_hash = ruby.hash_new();
    for name in ctx.captures.names() {
        if let Some(value) = ctx.captures.get(name) {
            let text = value.get_text(ctx.input());
            let _ = captures_hash.aset(ruby.to_symbol(name.as_str()), text.as_ref());
        }
    }
    let _ = hash.aset(ruby.to_symbol("captures"), captures_hash);
    Some(hash.into_value_with(ruby))
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
