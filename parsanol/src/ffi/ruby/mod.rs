//! Ruby FFI module for direct Ruby object construction
//!
//! This module provides a minimal, focused API for parsing with Parsanol.
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
//!
//! Low-level APIs (internal use):
//! - `parse_batch` - Returns flat u64 array (for debugging/benchmarks)
//! - `parse_with_builder` - Streaming builder callback

#![allow(missing_docs)]

mod builder;
mod cache;
mod dynamic;
mod init;
mod normalize;
mod parser;
mod traits;
mod transform;

// Public API - what users actually need
pub use init::init;
pub use parser::{is_available, parse};

// Low-level (for debugging/benchmarks)
pub use builder::RubyBuilder;
pub use dynamic::{
    register_ruby_callback_with_global_registry, unregister_ruby_callback_from_global_registry,
    RubyDynamicCallback,
};
pub use normalize::normalize_ast;
pub use parser::{
    clear_grammar_cache, grammar_cache_capacity, grammar_cache_size, parse_batch,
    parse_with_builder,
};
pub use traits::RubyObject;
pub use transform::transform_ast;
