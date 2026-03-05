//! Bytecode VM Backend Implementation
//!
//! This module wraps the existing Bytecode VM parser to implement the
//! [`ParsingBackend`] trait.

use crate::portable::backend::traits::{BackendCharacteristics, BackendResult, ParsingBackend};
use crate::portable::bytecode::backend::{Backend as InnerBackend, Parser};
use crate::portable::grammar::Grammar;

/// Bytecode VM backend
///
/// This backend compiles the grammar to bytecode instructions and executes
/// them on a stack-based virtual machine. It uses less memory than Packrat
/// but can exhibit exponential time complexity for certain patterns.
///
/// # Characteristics
///
/// - **Time**: O(n) to O(2^n) depending on grammar structure
/// - **Memory**: O(d) where d = nesting depth
/// - **Risk**: Nested repetitions can cause exponential time
/// - **Streaming**: Supports partial input parsing
///
/// # When to Use
///
/// - Simple grammars without nested repetitions
/// - Memory-constrained environments (embedded, WASM)
/// - Very large files where Packrat's memory is prohibitive
/// - Log parsing and streaming protocols
///
/// # Warning
///
/// If your grammar has nested repetitions like `(a*)*`, use Packrat instead
/// to avoid exponential backtracking.
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::portable::backend::BytecodeBackend;
/// use parsanol::portable::backend::ParsingBackend;
///
/// let mut backend = BytecodeBackend::new();
/// let result = backend.parse(&grammar, input)?;
/// ```
#[derive(Debug, Default)]
pub struct BytecodeBackend {
    /// Whether to automatically fall back to Packrat for problematic grammars
    auto_fallback: bool,
}

impl BytecodeBackend {
    /// Create a new Bytecode backend
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable automatic fallback to Packrat for problematic grammars
    ///
    /// When enabled, the backend will analyze the grammar and automatically
    /// use Packrat if nested repetitions are detected.
    pub fn with_auto_fallback(mut self, enabled: bool) -> Self {
        self.auto_fallback = enabled;
        self
    }
}

impl ParsingBackend for BytecodeBackend {
    fn parse(&mut self, grammar: &Grammar, input: &str) -> BackendResult {
        let backend = if self.auto_fallback {
            InnerBackend::Auto
        } else {
            InnerBackend::Bytecode
        };

        let mut parser = Parser::new(grammar.clone(), backend);
        parser.parse(input).map(|result| result.value)
    }

    fn name(&self) -> &'static str {
        "bytecode"
    }

    fn characteristics(&self) -> BackendCharacteristics {
        BackendCharacteristics::BYTECODE
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn is_safe_for_all_grammars(&self) -> bool {
        // Only safe with auto-fallback enabled
        self.auto_fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytecode_backend_name() {
        let backend = BytecodeBackend::new();
        assert_eq!(backend.name(), "bytecode");
    }

    #[test]
    fn test_bytecode_backend_characteristics() {
        let backend = BytecodeBackend::new();
        let chars = backend.characteristics();
        assert!(!chars.uses_memoization);
        assert!(chars.supports_streaming);
    }

    #[test]
    fn test_bytecode_auto_fallback() {
        let backend = BytecodeBackend::new().with_auto_fallback(true);
        assert!(backend.is_safe_for_all_grammars());
    }
}
