//! FFI (Foreign Function Interface) module for Parsanol
//!
//! This module provides bindings for using Parsanol from other languages.
//! Each binding type is behind a feature flag to minimize compile times
//! and dependencies.
//!
//! # Module Organization
//!
//! ```text
//! ffi/
//! ├── mod.rs         # This file - re-exports and feature gating
//! ├── shared.rs      # Shared utilities (flatten_ast_to_u64, tags)
//! ├── ruby/          # Ruby FFI (feature = "ruby")
//! ├── wasm/          # WASM bindings (feature = "wasm")
//! └── c/             # C ABI (always available)
//! ```
//!
//! # Feature Flags
//!
//! - `ruby` - Enable Ruby FFI bindings via magnus
//! - `wasm` - Enable WebAssembly bindings
//!
//! # Usage
//!
//! ## C ABI
//!
//! The C ABI is always available and provides a stable interface:
//!
//! ```c
//! #include <parsanol.h>
//! ParsanolGrammar* grammar = parsanol_grammar_new(json);
//! char* result = parsanol_parse(grammar, input);
//! parsanol_grammar_free(grammar);
//! ```
//!
//! ## Ruby FFI
//!
//! Enable with `--features ruby`:
//!
//! ```ruby
//! require 'parsanol'
//! parser = Parsanol::Parser.new(grammar_json)
//! result = parser.parse(input)
//! ```
//!
//! ## WASM
//!
//! Enable with `--features wasm`:
//!
//! ```javascript
//! import { WasmParser } from 'parsanol';
//! const parser = new WasmParser(grammarJson);
//! const result = parser.parse(input);
//! ```

// Shared utilities - always available
pub mod shared;

// C ABI - always available
pub mod c;

// Ruby FFI - feature gated
#[cfg(feature = "ruby")]
pub mod ruby;

// WASM bindings - feature gated
#[cfg(feature = "wasm")]
pub mod wasm;

// Re-export shared utilities for convenience
pub use shared::{
    flatten_ast, flatten_ast_to_u64, parse_to_flat, TAG_ARRAY_END, TAG_ARRAY_START, TAG_BOOL,
    TAG_FLOAT, TAG_HASH_END, TAG_HASH_KEY, TAG_HASH_START, TAG_INLINE_STRING, TAG_INT, TAG_NIL,
    TAG_STRING, TAG_SYMBOL, write_symbol,
};
