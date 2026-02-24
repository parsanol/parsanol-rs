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
    /// This trait can be implemented manually or automatically using
    /// `#[derive(RubyObject)]` from the `parsanol-ruby-derive` crate.
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
            Ok((*self as bool).into_value_with(ruby))
        }
    }

    impl RubyObject for String {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            Ok(ruby.str_new(self).as_value())
        }
    }

    impl RubyObject for &str {
        fn to_ruby(&self, ruby: &Ruby) -> Result<Value, Error> {
            Ok(ruby.str_new(*self).as_value())
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

    // Re-export derive macro if available
    #[cfg(feature = "parsanol-ruby-derive")]
    pub use parsanol_ruby_derive::RubyObject;

    // --- Native extension functions ---

    use magnus::{function, value::ReprValue, IntoValue, Module, RArray, TryConvert};
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

    #[magnus::init]
    pub fn init(ruby: &Ruby) -> Result<(), Error> {
        let module = ruby.define_module("Parsanol")?;
        let native_module = module.define_module("Native")?;

        native_module.define_module_function("is_available", function!(is_available, 0))?;
        native_module.define_module_function("parse_batch", function!(parse_batch, 2))?;
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
