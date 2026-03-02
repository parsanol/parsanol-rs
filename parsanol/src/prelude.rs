//! Prelude module for convenient imports
//!
//! This module re-exports the most commonly used types and traits from parsanol.
//! Importing this module with a wildcard import brings the core types into scope:
//!
//! ```
//! use parsanol::prelude::*;
//! ```
//!
//! # Re-exported Items
//!
//! ## Core Types
//! - [`AstArena`] - Arena allocator for AST nodes
//! - [`AstNode`] - AST node type (16 bytes, Copy)
//! - [`Grammar`] - PEG grammar definition
//! - [`PortableParser`] - Main parser type
//! - [`ParseError`] - Parse error type
//! - [`ParseResult`] - Result of a parse operation
//!
//! ## Parser DSL
//! - [`str()`] - Match a literal string
//! - [`re()`] - Match a regex pattern
//! - [`seq()`] - Match a sequence of patterns
//! - [`choice()`] - Match one of several patterns
//! - [`any()`] - Match any single character
//! - [`ref_()`] - Reference to another rule
//! - [`dynamic()`] - Dynamic pattern matching
//! - [`GrammarBuilder`] - Builder for constructing grammars
//! - [`Parslet`] - Trait for parslet types
//! - [`ParsletExt`] - Extension trait for parslet combinators
//!
//! ## Infix Parsing
//! - [`infix()`] - Create infix expression parser
//! - [`InfixBuilder`] - Builder for infix expressions
//! - [`Operator`] - Operator definition
//! - [`Assoc`] - Operator associativity
//! - [`PrecedenceClimber`] - Precedence climbing parser
//!
//! ## Error Handling
//! - [`RichError`] - Rich, tree-structured error
//! - [`ErrorBuilder`] - Builder for rich errors
//! - [`Span`] - Source code span
//!
//! ## Transforms
//! - [`Transform`] - Transform trait
//! - [`Value`] - Dynamic value type
//! - [`DirectTransform`] - Direct transform type

// ============================================================================
// Core Types
// ============================================================================

pub use crate::portable::{AstArena, AstNode, Grammar, ParseError, ParseResult, PortableParser};

// ============================================================================
// Parser DSL
// ============================================================================

pub use crate::portable::parser_dsl::{
    any, choice, dynamic, re, ref_, seq, str, GrammarBuilder, Parslet, ParsletExt,
};

// ============================================================================
// Infix Parsing
// ============================================================================

pub use crate::portable::infix::{infix, Assoc, InfixBuilder, Operator, PrecedenceClimber};

// ============================================================================
// Error Handling
// ============================================================================

pub use crate::portable::error::{ErrorBuilder, RichError, Span};

// ============================================================================
// Transforms
// ============================================================================

pub use crate::portable::transform::{DirectTransform, Transform, Value};
