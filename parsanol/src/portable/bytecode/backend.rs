//! Backend abstraction for parsanol
//!
//! This module provides a unified interface for different parsing backends.
//! The bytecode VM and packrat memoization backends share the same API,
//! allowing users to choose the best backend for their grammar.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌──────────────┐     ┌─────────────┐
//! │   Grammar   │────▶│    Backend   │────▶│  ParseResult│
//! └─────────────┘     └──────────────┘     └─────────────┘
//!                            │
//!              ┌─────────────┼─────────────┐
//!              ▼             ▼             ▼
//!        ┌─────────┐   ┌──────────┐   ┌─────────┐
//!        │ Packrat │   │ Bytecode │   │   Auto  │
//!        └─────────┘   └──────────┘   └─────────┘
//! ```
//!
//! # When to Use Each Backend
//!
//! | Use Bytecode When...      | Use Packrat When...       |
//! |---------------------------|---------------------------|
//! | Linear patterns           | Nested repetitions        |
//! | Memory constrained        | Heavy backtracking        |
//! | Simple grammars           | Incremental parsing       |
//! | Streaming (future)        | Predictable O(n) needed   |

// Re-export unified types from portable::backend
pub use crate::portable::backend::{Backend, GrammarAnalysis};

use crate::portable::arena::AstArena;
use crate::portable::ast::{ParseError, ParseResult};
use crate::portable::bytecode::compiler::Compiler;
use crate::portable::bytecode::vm::{BytecodeVM, VMConfig};
use crate::portable::grammar::Grammar;
use crate::portable::parser::PortableParser;

/// Unified parser with backend selection
pub struct Parser {
    grammar: Grammar,
    backend: Backend,
    vm_config: VMConfig,
    analysis: Option<GrammarAnalysis>,
}

impl Parser {
    /// Create a new parser with the given grammar and backend
    #[inline]
    pub fn new(grammar: Grammar, backend: Backend) -> Self {
        Self {
            grammar,
            backend,
            vm_config: VMConfig::default(),
            analysis: None,
        }
    }

    /// Create a parser with auto backend selection
    #[inline]
    pub fn auto(grammar: Grammar) -> Self {
        Self::new(grammar, Backend::Auto)
    }

    /// Create a parser with packrat backend
    #[inline]
    pub fn packrat(grammar: Grammar) -> Self {
        Self::new(grammar, Backend::Packrat)
    }

    /// Create a parser with bytecode backend
    #[inline]
    pub fn bytecode(grammar: Grammar) -> Self {
        Self::new(grammar, Backend::Bytecode)
    }

    /// Set VM configuration (for bytecode backend)
    #[inline]
    pub fn with_vm_config(mut self, config: VMConfig) -> Self {
        self.vm_config = config;
        self
    }

    /// Get the backend being used
    #[inline]
    pub fn backend(&self) -> Backend {
        self.backend
    }

    /// Get grammar analysis (lazy)
    pub fn analysis(&mut self) -> &GrammarAnalysis {
        if self.analysis.is_none() {
            self.analysis = Some(GrammarAnalysis::analyze(&self.grammar));
        }
        self.analysis.as_ref().unwrap()
    }

    /// Parse input and return the result
    pub fn parse(&mut self, input: &str) -> Result<ParseResult, ParseError> {
        let effective_backend = match self.backend {
            Backend::Auto => self.analysis().recommended_backend(),
            other => other,
        };

        match effective_backend {
            Backend::Packrat => self.parse_packrat(input),
            Backend::Bytecode => self.parse_bytecode(input),
            Backend::Auto => unreachable!(),
        }
    }

    /// Parse using packrat backend
    fn parse_packrat(&mut self, input: &str) -> Result<ParseResult, ParseError> {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&self.grammar, input, &mut arena);
        parser.parse_with_end_pos()
    }

    /// Parse using bytecode backend
    fn parse_bytecode(&mut self, input: &str) -> Result<ParseResult, ParseError> {
        // Compile grammar to program
        let program = Compiler::new(self.grammar.clone())
            .compile()
            .map_err(|e| ParseError::Internal {
                message: format!("Compilation error: {}", e),
            })?;

        // Execute program
        let mut arena = AstArena::for_input(input.len());
        let mut vm = BytecodeVM::new(&program, input, &mut arena, self.vm_config.clone());
        let result = vm.run()?;

        Ok(ParseResult {
            value: result.value,
            end_pos: result.end_pos,
        })
    }

    /// Get the effective backend (resolves Auto)
    pub fn effective_backend(&mut self) -> Backend {
        match self.backend {
            Backend::Auto => self.analysis().recommended_backend(),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests;
