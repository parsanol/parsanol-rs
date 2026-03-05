//! Parsing Backend Abstraction
//!
//! This module defines the [`ParsingBackend`] trait that allows different parsing
//! strategies to be used interchangeably. This enables:
//!
//! - **Extensibility**: Add custom backends (e.g., table-driven, recursive descent)
//! - **Testing**: Mock backends for unit testing
//! - **Runtime selection**: Switch backends based on grammar analysis
//!
//! # Available Backends
//!
//! - [`PackratBackend`] - Memoization-based, guaranteed O(n) time
//! - [`BytecodeBackend`] - Stack-based VM, lower memory, potential O(2^n)
//!
//! # Example
//!
//! ```rust,ignore
//! use parsanol::portable::backend::{ParsingBackend, PackratBackend, BytecodeBackend};
//!
//! // Use Packrat for predictable O(n)
//! let mut packrat = PackratBackend::new();
//! let result = packrat.parse(&grammar, input)?;
//!
//! // Use Bytecode for lower memory
//! let mut bytecode = BytecodeBackend::new();
//! let result = bytecode.parse(&grammar, input)?;
//! ```

mod traits;
mod packrat;
mod bytecode;
mod analysis;

pub use traits::{ParsingBackend, BackendCharacteristics, DynBackend, BackendResult};
pub use packrat::PackratBackend;
pub use bytecode::BytecodeBackend;
pub use analysis::{GrammarAnalysis, has_nested_repetition};

use crate::portable::grammar::Grammar;

/// Backend type enum for selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Backend {
    /// Packrat memoization backend
    ///
    /// - Time: Guaranteed O(n)
    /// - Memory: O(n × r) where r = number of rules
    /// - Best for: Complex grammars, nested repetitions, predictable latency
    Packrat,

    /// Bytecode VM backend
    ///
    /// - Time: O(n) to O(2^n) depending on grammar
    /// - Memory: O(d) where d = nesting depth
    /// - Best for: Simple patterns, memory-constrained environments
    Bytecode,

    /// Automatically select based on grammar analysis
    ///
    /// Analyzes the grammar and chooses the best backend:
    /// - Detects risky patterns (nested repetitions)
    /// - Selects Packrat for complex grammars, Bytecode for simple ones
    #[default]
    Auto,
}

impl Backend {
    /// Get backend name as a string
    pub fn name(&self) -> &'static str {
        match self {
            Backend::Packrat => "packrat",
            Backend::Bytecode => "bytecode",
            Backend::Auto => "auto",
        }
    }

    /// Check if this is the Packrat backend
    pub fn is_packrat(&self) -> bool {
        matches!(self, Backend::Packrat)
    }

    /// Check if this is the Bytecode backend
    pub fn is_bytecode(&self) -> bool {
        matches!(self, Backend::Bytecode)
    }

    /// Check if this is the Auto backend
    pub fn is_auto(&self) -> bool {
        matches!(self, Backend::Auto)
    }

    /// Get default backend for a grammar
    ///
    /// Uses the hard rule: nested repetitions → Packrat, otherwise → Bytecode
    pub fn default_for_grammar(grammar: &Grammar) -> Self {
        if has_nested_repetition(grammar) {
            Backend::Packrat
        } else {
            Backend::Bytecode
        }
    }
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
