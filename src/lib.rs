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

// Prelude module for convenient imports
pub mod prelude;

// Derive macro support (requires derive feature)
#[cfg(feature = "derive")]
pub mod derive;

// Conditional compilation for Ruby FFI
#[cfg(feature = "ruby")]
mod generic_lexer;

#[cfg(feature = "ruby")]
pub mod ruby_ffi;

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

// Conditional compilation for WASM bindings
#[cfg(feature = "wasm")]
mod wasm;
