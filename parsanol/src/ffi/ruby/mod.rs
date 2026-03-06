//! Ruby FFI module for direct Ruby object construction
//!
//! This module provides the `RubyObject` trait and related utilities
//! for Native (direct Ruby object construction via FFI).

#![allow(missing_docs)]

mod builder;
mod dynamic;
mod init;
mod parser;
mod traits;

// Re-export public API
pub use builder::RubyBuilder;
pub use dynamic::{
    register_ruby_callback_with_global_registry, unregister_ruby_callback_from_global_registry,
    RubyDynamicCallback,
};
pub use init::init;
pub use parser::{is_available, parse_batch, parse_to_ruby_objects, parse_with_builder};
pub use traits::RubyObject;
