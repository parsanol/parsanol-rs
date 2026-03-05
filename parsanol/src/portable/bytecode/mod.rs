//! Bytecode VM backend for Parsanol
//!
//! This module implements a bytecode virtual machine for PEG parsing,
//! inspired by Roberto Ierusalimschy's LPeg. The VM uses a backtracking
//! stack-based approach that is memory-efficient and provides predictable
//! performance for linear patterns.
//!
//! # Architecture
//!
//! ```text
//! Grammar (Atoms) ──► Compiler ──► Program (bytecode)
//!                                     │
//!                                     ▼
//! Input ──────────────────────────► VM ──► AstNode
//! ```
//!
//! # When to Use
//!
//! The bytecode VM is best suited for:
//! - Linear patterns (no nested repetitions)
//! - Memory-constrained environments
//! - Simple grammars
//! - Streaming parsing
//!
//! For grammars with heavy backtracking or nested repetitions,
//! the packrat backend may be more appropriate.

pub mod backend;
pub mod capture;
pub mod compiler;
pub mod error;
pub mod instruction;
pub mod optimizer;
pub mod pattern_analysis;
pub mod program;
pub mod vm;

pub use backend::{Backend, GrammarAnalysis, Parser};
pub use capture::{CaptureFrame, CaptureProcessor};
pub use compiler::{compile as compile_bytecode, CompileError, Compiler};
pub use error::{ErrorContext, ErrorReporter, ErrorTracker, Expected};
pub use instruction::{CaptureKind, Instruction, Opcode};
pub use optimizer::{OptimizationPass, PeepholeOptimizer};
pub use pattern_analysis::{FixedLenAnalysis, NullableAnalysis, PatternLength, PatternNullability};
pub use program::{CharSet, Program};
pub use vm::{parse_with_vm, BytecodeVM, VMConfig, VMResult};
