//! Parsanol - Highly Optimized Rust PEG Parser Library
//!
//! This is a generic PEG parser library that can be used for any language.
//! It provides:
//! - Core PEG parsing with packrat memoization
//! - Arena allocation for zero-copy AST construction
//! - Parser DSL for idiomatic grammar definition
//! - Generic lexer framework
//! - Rich error reporting with tree-structured errors
//! - Transformation system for converting parse trees to typed structs
//! - Infix expression parsing with precedence handling
//! - Developer tools (debug tracing, visualization)
//! - Optional Ruby FFI bindings
//! - Optional WASM bindings
//!
//! ## Quick Start
//!
//! ```rust
//! use parsanol::portable::{Grammar, PortableParser, AstArena};
//!
//! // Define grammar via JSON
//! let grammar_json = r#"{
//!     "atoms": [
//!         { "Str": { "pattern": "hello" } }
//!     ],
//!     "root": 0
//! }"#;
//!
//! let grammar: Grammar = serde_json::from_str(grammar_json).unwrap();
//! let input = "hello";
//!
//! let mut arena = AstArena::for_input(input.len());
//! let mut parser = PortableParser::new(&grammar, input, &mut arena);
//! let ast = parser.parse().unwrap();
//! ```
//!
//! ## Using the Parser DSL
//!
//! ```rust
//! use parsanol::portable::parser_dsl::*;
//!
//! let grammar = GrammarBuilder::new()
//!     .rule("greeting", str("hello").then(str("world")))
//!     .build();
//! ```
//!
//! ## Feature Flags
//!
//! - `ruby` - Enable Ruby FFI bindings via magnus
//! - `wasm` - Enable WebAssembly bindings
//! - `logging` - Enable debug logging using the `log` crate

// Lint configuration for production quality
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(clippy::all)]
#![allow(clippy::new_without_default)]
// Allow some pedantic lints that are too noisy
#![allow(clippy::module_inception)]
#![allow(clippy::redundant_closure)]

// Conditional compilation for Ruby FFI
#[cfg(feature = "ruby")]
mod generic_lexer;

// Portable core - available for both Ruby and future WASM
pub mod portable;

/// Re-export commonly used types for convenience
pub use portable::{
    // Debug tools
    debug::{GrammarVisualizer, ParseTrace, SourceFormatter, TreePrinter},
    // Rich errors
    error::{ErrorBuilder, RichError, Span},
    // Incremental parsing
    incremental::{DirtyRegion, DirtyRegionTracker, Edit, IncrementalParser, IncrementalResult},
    // Infix parsing
    infix::{infix, Assoc, InfixBuilder, Operator, PrecedenceClimber},
    // Parser DSL
    parser_dsl::{
        any, choice, dynamic, re, ref_, seq, str, Alternative2, Alternative3, Alternative4,
        Alternative5, GrammarBuilder, Parslet, ParsletExt, Sequence2, Sequence3, Sequence4,
        Sequence5,
    },
    // Streaming parsing
    streaming::{ChunkConfig, ChunkSource, StreamingError, StreamingParser, StreamingResult},
    // Transform
    transform::{ast_to_value, Transform, Value},
    AstArena,
    AstNode,
    Grammar,
    ParseError,
    PortableParser,
};

// Conditional compilation for Ruby FFI
#[cfg(feature = "ruby")]
pub mod ruby_ffi {
    #![allow(missing_docs)]

    //! Ruby FFI module for direct Ruby object construction
    //!
    //! This module provides the `RubyObject` trait and related utilities
    //! for Native (direct Ruby object construction via FFI).

    use magnus::{Error, Ruby, Value};

    /// Trait for types that can be converted to Ruby objects
    ///
    /// This trait enables direct Ruby object construction from Rust types,
    /// providing the fastest possible FFI path (Native).
    ///
    /// # Implementation
    ///
    /// # Example (Manual Implementation)
    ///
    /// ```rust,ignore
    /// use parsanol::ruby_ffi::RubyObject;
    /// use magnus::{Ruby, Value, Error, RClass, RObject};
    ///
    /// pub struct Number(i64);
    ///
    /// impl RubyObject for Number {
    ///     fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
    ///         let class: RClass = ruby.class("Calculator::Number")?;
    ///         class.new_instance((self.0,))
    ///     }
    /// }
    /// ```
    ///
    /// # Example (Derive Macro)
    ///
    /// ```rust,ignore
    /// use parsanol_ruby_derive::RubyObject;
    ///
    /// #[derive(RubyObject)]
    /// #[ruby_class("Calculator::Number")]
    /// pub struct Number(i64);
    /// ```
    pub trait RubyObject: Sized {
        /// Convert this Rust value to a Ruby object
        ///
        /// # Arguments
        ///
        /// * `ruby` - The Ruby interpreter handle
        ///
        /// # Returns
        ///
        /// A Ruby Value representing this object, or an error.
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error>;
    }

    // Implement RubyObject for primitive types
    impl RubyObject for i64 {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            Ok(ruby.integer_from_i64(*self).as_value())
        }
    }

    impl RubyObject for i32 {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            Ok(ruby.integer_from_i64(*self as i64).as_value())
        }
    }

    impl RubyObject for f64 {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            Ok(ruby.float_from_f64(*self).as_value())
        }
    }

    impl RubyObject for bool {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            Ok((*self).into_value_with(ruby))
        }
    }

    impl RubyObject for String {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            Ok(ruby.str_new(self).as_value())
        }
    }

    impl RubyObject for &str {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            Ok(ruby.str_new(self).as_value())
        }
    }

    impl<T: RubyObject> RubyObject for Option<T> {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            match self {
                Some(v) => v.to_ruby(ruby),
                None => {
                    // Return Ruby nil
                    let nil_val: Value = ruby.eval("nil")?;
                    Ok(nil_val)
                }
            }
        }
    }

    impl<T: RubyObject> RubyObject for Vec<T> {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            let ary = ruby.ary_new_capa(self.len() as _);
            for item in self {
                ary.push(item.to_ruby(ruby)?)?;
            }
            Ok(ary.as_value())
        }
    }

    // --- Streaming Builder Bridge ---

    use crate::portable::streaming_builder::{BuildError, BuildResult, StreamingBuilder};

    /// Ruby callback wrapper for streaming builder
    ///
    /// This wraps a Ruby object that implements the builder callback protocol.
    /// The Ruby object should respond to methods like `on_string`, `on_int`, etc.
    ///
    /// # Ruby Interface
    ///
    /// ```ruby
    /// class MyBuilder
    ///   def on_named_start(name); end
    ///   def on_named_end(name); end
    ///   def on_string(value, offset, length); end
    ///   def on_int(value); end
    ///   def on_float(value); end
    ///   def on_bool(value); end
    ///   def on_nil; end
    ///   def on_array_start(expected_len); end
    ///   def on_array_element(index); end
    ///   def on_array_end(actual_len); end
    ///   def on_hash_start(expected_len); end
    ///   def on_hash_key(key); end
    ///   def on_hash_value(key); end
    ///   def on_hash_end(actual_len); end
    ///   def on_start(input); end
    ///   def on_success; end
    ///   def on_error(message); end
    ///   def finish; end
    /// end
    /// ```
    pub struct RubyBuilder {
        /// The Ruby object implementing callbacks
        callback: Value,
    }

    impl RubyBuilder {
        /// Create a new Ruby builder wrapper
        ///
        /// # Arguments
        /// * `callback` - Ruby object with callback methods
        pub fn new(callback: Value) -> Self {
            Self { callback }
        }

        /// Call a method on the Ruby callback object
        fn call_method(&self, method: &str, args: &[Value]) -> BuildResult<()> {
            let _ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;

            if args.is_empty() {
                self.callback
                    .funcall::<&str, (), Value>(method, ())
                    .map(|_| ())
                    .map_err(|e| BuildError::Custom {
                        message: format!("Ruby callback error: {}", e),
                    })
            } else {
                self.callback
                    .funcall::<&str, &[Value], Value>(method, args)
                    .map(|_| ())
                    .map_err(|e| BuildError::Custom {
                        message: format!("Ruby callback error: {}", e),
                    })
            }
        }
    }

    impl StreamingBuilder for RubyBuilder {
        type Output = Value;

        fn on_named_start(&mut self, name: &str) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let name_val: Value = ruby.str_new(name).as_value();
            self.call_method("on_named_start", &[name_val])
        }

        fn on_named_end(&mut self, name: &str) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let name_val: Value = ruby.str_new(name).as_value();
            self.call_method("on_named_end", &[name_val])
        }

        fn on_string(&mut self, value: &str, offset: usize, length: usize) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let value_val: Value = ruby.str_new(value).as_value();
            let offset_val: Value = ruby.integer_from_i64(offset as i64).as_value();
            let length_val: Value = ruby.integer_from_i64(length as i64).as_value();
            self.call_method("on_string", &[value_val, offset_val, length_val])
        }

        fn on_int(&mut self, value: i64) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let value_val: Value = ruby.integer_from_i64(value).as_value();
            self.call_method("on_int", &[value_val])
        }

        fn on_float(&mut self, value: f64) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let value_val: Value = ruby.float_from_f64(value).as_value();
            self.call_method("on_float", &[value_val])
        }

        fn on_bool(&mut self, value: bool) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let value_val: Value = value.into_value_with(&ruby);
            self.call_method("on_bool", &[value_val])
        }

        fn on_nil(&mut self) -> BuildResult<()> {
            self.call_method("on_nil", &[])
        }

        fn on_array_start(&mut self, expected_len: Option<usize>) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let len_val: Value = match expected_len {
                Some(n) => ruby.integer_from_i64(n as i64).as_value(),
                None => ruby.qnil().as_value(),
            };
            self.call_method("on_array_start", &[len_val])
        }

        fn on_array_element(&mut self, index: usize) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let index_val: Value = ruby.integer_from_i64(index as i64).as_value();
            self.call_method("on_array_element", &[index_val])
        }

        fn on_array_end(&mut self, actual_len: usize) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let len_val: Value = ruby.integer_from_i64(actual_len as i64).as_value();
            self.call_method("on_array_end", &[len_val])
        }

        fn on_hash_start(&mut self, expected_len: Option<usize>) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let len_val: Value = match expected_len {
                Some(n) => ruby.integer_from_i64(n as i64).as_value(),
                None => ruby.qnil().as_value(),
            };
            self.call_method("on_hash_start", &[len_val])
        }

        fn on_hash_key(&mut self, key: &str) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let key_val: Value = ruby.str_new(key).as_value();
            self.call_method("on_hash_key", &[key_val])
        }

        fn on_hash_value(&mut self, key: &str) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let key_val: Value = ruby.str_new(key).as_value();
            self.call_method("on_hash_value", &[key_val])
        }

        fn on_hash_end(&mut self, actual_len: usize) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let len_val: Value = ruby.integer_from_i64(actual_len as i64).as_value();
            self.call_method("on_hash_end", &[len_val])
        }

        fn on_start(&mut self, input: &str) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let input_val: Value = ruby.str_new(input).as_value();
            self.call_method("on_start", &[input_val])
        }

        fn on_success(&mut self) -> BuildResult<()> {
            self.call_method("on_success", &[])
        }

        fn on_error(&mut self, error: &crate::portable::ast::ParseError) -> BuildResult<()> {
            let ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;
            let msg_val: Value = ruby.str_new(&error.to_string()).as_value();
            self.call_method("on_error", &[msg_val])
        }

        fn finish(&mut self) -> BuildResult<Value> {
            let _ruby = Ruby::get().map_err(|e| BuildError::Custom {
                message: format!("Ruby not available: {}", e),
            })?;

            self.callback
                .funcall::<&str, (), Value>("finish", ())
                .map_err(|e| BuildError::Custom {
                    message: format!("Ruby callback error: {}", e),
                })
        }
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
            let cache = GRAMMAR_CACHE.lock().unwrap();
            if let Some(cached) = cache.get(&hash) {
                cached.clone()
            } else {
                drop(cache);
                let grammar: Grammar = serde_json::from_str(&grammar_json)
                    .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;
                let mut cache = GRAMMAR_CACHE.lock().unwrap();
                cache.insert(hash, grammar.clone());
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

    // --- Native extension functions ---

    use magnus::{
        function, value::ReprValue, Class, IntoValue, Module, RArray, RClass, TryConvert,
    };
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    use crate::portable::ffi::flatten_ast_to_u64;
    use crate::portable::{AstArena, AstNode, Grammar, PortableParser};

    // Thread-safe global grammar cache
    static GRAMMAR_CACHE: Lazy<Mutex<hashbrown::HashMap<u64, Grammar>>> =
        Lazy::new(|| Mutex::new(hashbrown::HashMap::new()));

    fn hash_string(s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
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
            let cache = GRAMMAR_CACHE.lock().unwrap();
            if let Some(cached) = cache.get(&hash) {
                cached.clone()
            } else {
                // Drop lock before parsing
                drop(cache);

                let grammar: Grammar = serde_json::from_str(&grammar_json)
                    .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;

                let mut cache = GRAMMAR_CACHE.lock().unwrap();
                cache.insert(hash, grammar.clone());
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
            let cache = GRAMMAR_CACHE.lock().unwrap();
            if let Some(cached) = cache.get(&hash) {
                cached.clone()
            } else {
                // Drop lock before parsing
                drop(cache);

                let grammar: Grammar = serde_json::from_str(&grammar_json)
                    .map_err(|e| Error::new(ruby.exception_arg_error(), e.to_string()))?;

                let mut cache = GRAMMAR_CACHE.lock().unwrap();
                cache.insert(hash, grammar.clone());
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

    /// Create a cached lexer from token definitions
    pub fn create_lexer(definitions: Value) -> Result<Value, Error> {
        let ruby = Ruby::get().unwrap();

        // Convert to Ruby array and iterate
        let ary: RArray = TryConvert::try_convert(definitions)?;
        let len = ary.len();

        let mut token_defs = Vec::new();

        for i in 0..len {
            let def: Value = ary.entry(i as isize)?;
            let hash: magnus::RHash = TryConvert::try_convert(def)?;

            let name: String = match hash.lookup("name")? {
                Some(v) => TryConvert::try_convert(v)?,
                None => return Err(Error::new(ruby.exception_arg_error(), "missing name")),
            };

            let pattern: String = match hash.lookup("pattern")? {
                Some(v) => TryConvert::try_convert(v)?,
                None => return Err(Error::new(ruby.exception_arg_error(), "missing pattern")),
            };

            let priority_val: Value = hash.lookup("priority")?;
            let priority: i32 = if priority_val.is_nil() {
                0
            } else {
                TryConvert::try_convert(priority_val).unwrap_or(0)
            };

            let ignore_val: Value = hash.lookup("ignore")?;
            let ignore: bool = if ignore_val.is_nil() {
                false
            } else {
                TryConvert::try_convert(ignore_val).unwrap_or(false)
            };

            token_defs.push(crate::generic_lexer::TokenDef {
                name,
                pattern,
                priority,
                ignore,
            });
        }

        let lexer_id = crate::generic_lexer::create_lexer(token_defs)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e))?;

        Ok(lexer_id.into_value_with(&ruby))
    }

    /// Tokenize input using a cached lexer
    pub fn tokenize_with_lexer(lexer_id: usize, input: String) -> Result<Value, Error> {
        let ruby = Ruby::get().unwrap();

        let tokens = crate::generic_lexer::tokenize_with_lexer(lexer_id, &input)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e))?;

        let ruby_array = ruby.ary_new_capa(tokens.len() as _);

        for token in tokens {
            let hash = ruby.hash_new();
            hash.aset("type", token.token_type)?;
            hash.aset("value", token.value)?;

            let loc_hash = ruby.hash_new();
            loc_hash.aset("line", token.location.line as i64)?;
            loc_hash.aset("column", token.location.column as i64)?;
            loc_hash.aset("offset", token.location.offset as i64)?;
            hash.aset("location", loc_hash)?;

            ruby_array.push(hash)?;
        }

        Ok(ruby_array.into_value_with(&ruby))
    }

    /// Remove a cached lexer
    pub fn drop_lexer(lexer_id: usize) -> Result<Value, Error> {
        let ruby = Ruby::get().unwrap();
        let removed = crate::generic_lexer::drop_lexer(lexer_id);
        Ok(removed.into_value_with(&ruby))
    }

    /// Initialize the Ruby native extension module
    #[magnus::init]
    pub fn init(ruby: &Ruby) -> Result<(), Error> {
        let module = ruby.define_module("Parsanol")?;
        let native_module = module.define_module("Native")?;

        native_module.define_module_function("is_available", function!(is_available, 0))?;
        native_module.define_module_function("parse_batch", function!(parse_batch, 2))?;
        native_module
            .define_module_function("parse_to_ruby_objects", function!(parse_to_ruby_objects, 2))?;
        native_module
            .define_module_function("parse_with_builder", function!(parse_with_builder, 3))?;
        native_module.define_module_function("create_lexer", function!(create_lexer, 1))?;
        native_module
            .define_module_function("tokenize_with_lexer", function!(tokenize_with_lexer, 2))?;
        native_module.define_module_function("drop_lexer", function!(drop_lexer, 1))?;

        Ok(())
    }
}

// Conditional compilation for WASM bindings
#[cfg(feature = "wasm")]
mod wasm;
