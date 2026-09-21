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
use crate::portable::bytecode::program::Program;
use crate::portable::bytecode::vm::{BytecodeVM, VMConfig};
use crate::portable::grammar::Grammar;
use crate::portable::parser::PortableParser;

/// Unified parser with backend selection
pub struct Parser {
    grammar: Grammar,
    backend: Backend,
    vm_config: VMConfig,
    analysis: Option<GrammarAnalysis>,
    /// Compile-once program (#90): when set, the bytecode backend uses
    /// it instead of recompiling per parse.
    program: Option<Program>,
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
            program: None,
        }
    }

    /// Attach a precompiled program (#90): the bytecode backend uses
    /// it as-is, amortizing compilation across parses. The grammar
    /// passed to `Parser::new` must be the one the program came from.
    #[inline]
    pub fn with_program(mut self, program: Program) -> Self {
        self.program = Some(program);
        self
    }

    /// Compile (or return the memoized) program for the bytecode
    /// backend, so callers can keep one `Program` alive compile-once.
    pub fn compile_program(&mut self) -> Result<&Program, ParseError> {
        if self.program.is_none() {
            self.program = Some(Compiler::new(self.grammar.clone()).compile().map_err(|e| {
                ParseError::Internal {
                    message: format!("Compilation error: {}", e),
                }
            })?);
        }
        Ok(self.program.as_ref().expect("program compiled"))
    }

    /// Parse into a caller-owned arena (#90): pool-backed results
    /// (StringRef/Array/Hash indexes) stay consumable by arena-based
    /// utilities, matching `PortableParser`'s ownership model.
    pub fn parse_into(
        &mut self,
        arena: &mut AstArena,
        input: &str,
    ) -> Result<ParseResult, ParseError> {
        let effective_backend = match self.backend {
            Backend::Auto => self.analysis().recommended_backend(),
            other => other,
        };

        match effective_backend {
            Backend::Packrat => {
                let mut parser = PortableParser::new(&self.grammar, input, arena);
                parser.parse_with_end_pos()
            }
            Backend::Bytecode => {
                let vm_config = self.vm_config.clone();
                let program = self.compile_program()?;
                let mut vm = BytecodeVM::new(program, input, arena, vm_config);
                let result = vm.run()?;
                Ok(ParseResult {
                    value: result.value,
                    end_pos: result.end_pos,
                    capture_state: None,
                })
            }
            Backend::Auto => unreachable!(),
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
        let mut arena = AstArena::for_input(input.len());
        self.parse_into(&mut arena, input)
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
