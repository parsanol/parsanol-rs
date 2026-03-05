//! Parsing Backend Trait Definition
//!
//! This module defines the core trait that all parsing backends must implement.

use crate::portable::ast::{AstNode, ParseError};
use crate::portable::grammar::Grammar;

/// Result type for parsing operations
pub type BackendResult = Result<AstNode, ParseError>;

/// Characteristics of a parsing backend
///
/// Used for documentation, debugging, and automatic backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCharacteristics {
    /// Time complexity description
    ///
    /// Examples: "O(n)", "O(n) to O(2^n)", "O(n × k)"
    pub time_complexity: &'static str,

    /// Memory complexity description
    ///
    /// Examples: "O(n × r)", "O(d)", "O(1)"
    pub memory_complexity: &'static str,

    /// Whether this backend uses memoization
    pub uses_memoization: bool,

    /// Whether this backend supports streaming (partial input)
    pub supports_streaming: bool,

    /// Whether this backend supports incremental re-parsing
    pub supports_incremental: bool,

    /// Whether this backend is safe for all grammars (no exponential risk)
    pub safe_for_all_grammars: bool,
}

impl BackendCharacteristics {
    /// Packrat characteristics
    pub const PACKRAT: Self = Self {
        time_complexity: "O(n)",
        memory_complexity: "O(n × r)",
        uses_memoization: true,
        supports_streaming: false,
        supports_incremental: true,
        safe_for_all_grammars: true,
    };

    /// Bytecode characteristics
    pub const BYTECODE: Self = Self {
        time_complexity: "O(n) to O(2^n)",
        memory_complexity: "O(d)",
        uses_memoization: false,
        supports_streaming: true,
        supports_incremental: false,
        safe_for_all_grammars: false,
    };
}

/// Parsing backend trait
///
/// All parsing backends (Packrat, Bytecode, custom) implement this trait.
/// This enables:
///
/// - **Polymorphism**: Swap backends at runtime
/// - **Testing**: Create mock backends for unit tests
/// - **Extensibility**: Add new backends without modifying core code
///
/// # Implementation Notes
///
/// Backends should be stateless between parses. Any state (like memoization
/// caches) should be created fresh for each `parse()` call or stored in
/// a separate context object.
///
/// # Example Implementation
///
/// ```rust,ignore
/// use parsanol::portable::backend::{ParsingBackend, BackendCharacteristics};
/// use parsanol::portable::grammar::Grammar;
/// use parsanol::portable::ast::{AstNode, ParseError, ParseResult};
///
/// struct MyCustomBackend;
///
/// impl ParsingBackend for MyCustomBackend {
///     fn parse(&mut self, grammar: &Grammar, input: &str) -> ParseResult<AstNode> {
///         // Custom parsing logic here
///         todo!()
///     }
///
///     fn name(&self) -> &'static str {
///         "my-custom"
///     }
///
///     fn characteristics(&self) -> BackendCharacteristics {
///         BackendCharacteristics {
///             time_complexity: "O(n log n)",
///             memory_complexity: "O(n)",
///             uses_memoization: false,
///             supports_streaming: true,
///             supports_incremental: false,
///             safe_for_all_grammars: true,
///         }
///     }
/// }
/// ```
pub trait ParsingBackend {
    /// Parse input using the given grammar
    ///
    /// # Arguments
    ///
    /// * `grammar` - The compiled grammar to use
    /// * `input` - The input string to parse
    ///
    /// # Returns
    ///
    /// * `Ok(AstNode)` - The parsed AST root
    /// * `Err(ParseError)` - Parse failure with error details
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut backend = PackratBackend::new();
    /// let ast = backend.parse(&grammar, "hello world")?;
    /// ```
    fn parse(&mut self, grammar: &Grammar, input: &str) -> BackendResult;

    /// Get the backend name
    ///
    /// Used for logging, debugging, and error messages.
    fn name(&self) -> &'static str;

    /// Get backend characteristics
    ///
    /// Returns information about time/memory complexity and capabilities.
    fn characteristics(&self) -> BackendCharacteristics;

    /// Check if this backend supports streaming
    ///
    /// Streaming backends can parse partial input and continue later.
    fn supports_streaming(&self) -> bool {
        self.characteristics().supports_streaming
    }

    /// Check if this backend supports incremental parsing
    ///
    /// Incremental backends can reuse cached results when input changes.
    fn supports_incremental(&self) -> bool {
        self.characteristics().supports_incremental
    }

    /// Check if this backend is safe for all grammars
    ///
    /// Safe backends have no exponential time risk.
    fn is_safe_for_all_grammars(&self) -> bool {
        self.characteristics().safe_for_all_grammars
    }

    /// Parse with a pre-allocated arena
    ///
    /// This is an optional optimization for reusing memory across parses.
    fn parse_with_arena(
        &mut self,
        grammar: &Grammar,
        input: &str,
        arena: &mut crate::portable::arena::AstArena,
    ) -> BackendResult {
        // Default implementation ignores the arena
        let _ = arena;
        self.parse(grammar, input)
    }
}

/// Dynamic backend dispatch
///
/// Use this when you need to store or pass around different backend types.
pub type DynBackend = Box<dyn ParsingBackend + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packrat_characteristics() {
        let chars = BackendCharacteristics::PACKRAT;
        assert!(chars.uses_memoization);
        assert!(chars.safe_for_all_grammars);
        assert!(chars.supports_incremental);
        assert!(!chars.supports_streaming);
    }

    #[test]
    fn test_bytecode_characteristics() {
        let chars = BackendCharacteristics::BYTECODE;
        assert!(!chars.uses_memoization);
        assert!(!chars.safe_for_all_grammars);
        assert!(chars.supports_streaming);
        assert!(!chars.supports_incremental);
    }
}
