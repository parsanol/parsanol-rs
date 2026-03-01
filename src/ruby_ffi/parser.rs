//! Parser functions for Ruby FFI

use crate::portable::ffi::flatten_ast_to_u64;
use crate::portable::{AstArena, AstNode, Grammar, PortableParser};
use magnus::{value::ReprValue, Class, Error, IntoValue, RClass, Ruby, Value};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use super::builder::RubyBuilder;

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

/// Parse using batch FFI - returns flat array instead of Ruby objects
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

/// Parse and return direct Ruby objects via FFI (ZeroCopy mode)
///
/// This function parses the input and constructs Ruby objects directly via magnus,
/// bypassing the u64 serialization step. For InputRef nodes, it directly constructs
/// `Parsanol::Slice` objects, eliminating the need for the Ruby-side `convert_slices`
/// conversion.
///
/// # Performance
///
/// This is the fastest FFI mode, providing 8-10% improvement over the batch mode
/// with slice conversion.
///
/// # Arguments
///
/// * `grammar_json` - JSON string containing the grammar definition
/// * `input` - Input string to parse
///
/// # Returns
///
/// Ruby Value containing the parsed AST with `Parsanol::Slice` objects for
/// input references.
pub fn parse_to_ruby_objects(grammar_json: String, input: String) -> Result<Value, Error> {
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

    // Convert AST directly to Ruby objects with Slice construction
    ast_node_to_ruby(&ast, &arena, &input, &ruby)
}

/// Parse with a Ruby builder callback
///
/// This function parses the input using a Ruby object that implements
/// the builder callback protocol. The builder receives streaming events
/// during parsing.
///
/// # Arguments
/// * `grammar_json` - JSON string containing the grammar definition
/// * `input` - Input string to parse
/// * `builder` - Ruby object implementing builder callbacks
///
/// # Returns
/// The result of calling `builder.finish`
///
/// # Example (Ruby)
///
/// ```ruby
/// class StringCollector
///   def initialize
///     @strings = []
///   end
///
///   def on_string(value, offset, length)
///     @strings << value
///   end
///
///   def finish
///     @strings
///   end
/// end
///
/// result = Parsanol::Native.parse_with_builder(grammar_json, input, StringCollector.new)
/// ```
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

/// Recursively convert an AstNode to a Ruby Value
///
/// For InputRef nodes, this directly constructs `Parsanol::Slice` objects
/// instead of returning intermediate Hash markers.
fn ast_node_to_ruby(
    node: &AstNode,
    arena: &AstArena,
    input: &str,
    ruby: &Ruby,
) -> Result<Value, Error> {
    match node {
        AstNode::Nil => Ok(ruby.qnil().as_value()),

        AstNode::Bool(b) => Ok((*b).into_value_with(ruby)),

        AstNode::Int(n) => Ok(ruby.integer_from_i64(*n).as_value()),

        AstNode::Float(f) => Ok(ruby.float_from_f64(*f).as_value()),

        AstNode::StringRef { pool_index } => {
            let (s, _, _) = arena.get_string_parts(*pool_index as usize);
            Ok(ruby.str_new(s).as_value())
        }

        AstNode::InputRef { offset, length } => {
            // Get the slice content from the input
            let start = *offset as usize;
            let end = start + (*length as usize);
            let slice_str = if end <= input.len() {
                &input[start..end]
            } else {
                // Fallback for edge cases
                ""
            };

            // Get Parsanol::Slice class via const_get
            // First get the Parsanol module, then get the Slice class from it
            let parsanol_module: magnus::RModule =
                ruby.class_object().const_get("Parsanol").map_err(|e| {
                    Error::new(
                        ruby.exception_runtime_error(),
                        format!("Parsanol module not found: {}", e),
                    )
                })?;

            let slice_class: RClass = parsanol_module.const_get("Slice").map_err(|e| {
                Error::new(
                    ruby.exception_runtime_error(),
                    format!("Parsanol::Slice class not found: {}", e),
                )
            })?;

            // Create arguments: (bytepos, string)
            let offset_val = ruby.integer_from_i64(*offset as i64);
            let str_val = ruby.str_new(slice_str);

            slice_class.new_instance((offset_val, str_val))
        }

        AstNode::Array { pool_index, length } => {
            let items = arena.get_array(*pool_index as usize, *length as usize);
            let ary = ruby.ary_new_capa(items.len() as _);

            for item in items {
                let ruby_item = ast_node_to_ruby(&item, arena, input, ruby)?;
                ary.push(ruby_item)?;
            }

            Ok(ary.as_value())
        }

        AstNode::Hash { pool_index, length } => {
            let items = arena.get_hash_items(*pool_index as usize, *length as usize);
            let hash = ruby.hash_new();

            for (key, value) in items {
                let key_val = ruby.str_new(&key).as_value();
                let value_val = ast_node_to_ruby(&value, arena, input, ruby)?;
                hash.aset(key_val, value_val)?;
            }

            Ok(hash.as_value())
        }
    }
}
