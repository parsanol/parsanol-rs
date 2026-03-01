//! Ruby FFI module for direct Ruby object construction
//!
//! This module provides the `RubyObject` trait and related utilities
//! for Native (direct Ruby object construction via FFI).

#![allow(missing_docs)]

mod builder;
mod init;
mod lexer;
mod parser;
mod traits;

// Re-export public API
pub use builder::RubyBuilder;
pub use init::init;
pub use lexer::{create_lexer, drop_lexer, tokenize_with_lexer};
pub use parser::{is_available, parse_batch, parse_to_ruby_objects, parse_with_builder};
pub use traits::RubyObject;
