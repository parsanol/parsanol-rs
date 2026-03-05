//! Packrat Backend Implementation
//!
//! This module wraps the existing Packrat parser to implement the
//! [`ParsingBackend`] trait.

use crate::portable::arena::AstArena;
use crate::portable::backend::traits::{BackendCharacteristics, BackendResult, ParsingBackend};
use crate::portable::grammar::Grammar;
use crate::portable::parser::PortableParser;

/// Packrat memoization backend
///
/// This backend uses Packrat memoization to guarantee O(n) parsing time
/// for all grammars. It stores parse results in a memoization table
/// keyed by (position, rule) pairs.
///
/// # Characteristics
///
/// - **Time**: Guaranteed O(n) where n = input length
/// - **Memory**: O(n × r) where r = number of rules
/// - **Safe**: No exponential time risk
/// - **Incremental**: Supports reusing cache between parses
///
/// # When to Use
///
/// - Complex grammars with nested repetitions
/// - Safety-critical applications requiring predictable latency
/// - When memory is not constrained
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::portable::backend::PackratBackend;
/// use parsanol::portable::backend::ParsingBackend;
///
/// let mut backend = PackratBackend::new();
/// let result = backend.parse(&grammar, input)?;
/// ```
#[derive(Debug, Default)]
pub struct PackratBackend {
    max_recursion_depth: usize,
    timeout_ms: u64,
}

impl PackratBackend {
    /// Create a new Packrat backend with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum recursion depth
    pub fn with_max_recursion_depth(mut self, depth: usize) -> Self {
        self.max_recursion_depth = depth;
        self
    }

    /// Set timeout in milliseconds
    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }
}

impl ParsingBackend for PackratBackend {
    fn parse(&mut self, grammar: &Grammar, input: &str) -> BackendResult {
        let estimated_nodes = input.len() / 10;
        let mut arena = AstArena::for_input(estimated_nodes);

        let mut parser = PortableParser::new(grammar, input, &mut arena);

        if self.max_recursion_depth > 0 {
            parser.set_max_recursion_depth(self.max_recursion_depth);
        }

        if self.timeout_ms > 0 {
            parser.set_timeout_ms(self.timeout_ms);
        }

        parser.parse()
    }

    fn name(&self) -> &'static str {
        "packrat"
    }

    fn characteristics(&self) -> BackendCharacteristics {
        BackendCharacteristics::PACKRAT
    }

    fn parse_with_arena(
        &mut self,
        grammar: &Grammar,
        input: &str,
        arena: &mut AstArena,
    ) -> BackendResult {
        let mut parser = PortableParser::new(grammar, input, arena);

        if self.max_recursion_depth > 0 {
            parser.set_max_recursion_depth(self.max_recursion_depth);
        }

        if self.timeout_ms > 0 {
            parser.set_timeout_ms(self.timeout_ms);
        }

        parser.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::parser_dsl::GrammarBuilder;

    #[test]
    fn test_packrat_backend_name() {
        let backend = PackratBackend::new();
        assert_eq!(backend.name(), "packrat");
    }

    #[test]
    fn test_packrat_backend_characteristics() {
        let backend = PackratBackend::new();
        let chars = backend.characteristics();
        assert!(chars.uses_memoization);
        assert!(chars.safe_for_all_grammars);
    }
}
