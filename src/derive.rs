//! Derive macro support for parsanol
//!
//! This module provides error types and utilities for the `FromAst` derive macro.

use std::fmt;

/// Error type for FromAst conversions
#[derive(Debug, Clone)]
pub enum FromAstError {
    /// The AST node type doesn't match what was expected
    TypeMismatch {
        /// The expected type name
        expected: &'static str,
        /// The actual type name found
        actual: &'static str,
    },
    /// A required field is missing from a hash
    MissingField(String),
    /// The conversion failed for another reason
    ConversionError,
    /// Expected an array but got something else
    ExpectedArray,
    /// Expected a hash but got something else
    ExpectedHash,
    /// Unknown tag in enum matching
    UnknownTag,
    /// Custom error message
    Custom(String),
}

impl fmt::Display for FromAstError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeMismatch { expected, actual } => {
                write!(f, "type mismatch: expected {}, got {}", expected, actual)
            }
            Self::MissingField(field) => {
                write!(f, "missing field: {}", field)
            }
            Self::ConversionError => {
                write!(f, "conversion error")
            }
            Self::ExpectedArray => {
                write!(f, "expected array")
            }
            Self::ExpectedHash => {
                write!(f, "expected hash")
            }
            Self::UnknownTag => {
                write!(f, "unknown tag")
            }
            Self::Custom(msg) => {
                write!(f, "{}", msg)
            }
        }
    }
}

impl std::error::Error for FromAstError {}

// Re-export derive macros when the derive feature is enabled
#[cfg(feature = "derive")]
pub use parsanol_derive::FromAst;
