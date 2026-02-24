//! Parallel parsing support
//!
//! This module provides parallel parsing capabilities for multi-file scenarios.
//!
//! # Overview
//!
//! For parsing multiple independent files (e.g., EXPRESS schemas),
//! parallel parsing can provide linear speedup per CPU core.
//!
//! # Feature Flag
//!
//! This module is only available when the `parallel` feature is enabled:
//!
//! ```toml
//! [dependencies]
//! parsanol = { version = "0.1", features = ["parallel"] }
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use parsanol::portable::{Grammar, parallel::parse_batch_parallel};
//!
//! let grammar = /* ... */;
//! let inputs = vec!["file1.exp", "file2.exp", /* ... */];
//!
//! // Parse all files in parallel
//! let results = parse_batch_parallel(&grammar, &inputs);
//!
//! // Results are in same order as inputs
//! for (i, result) in results.iter().enumerate() {
//!     match result {
//!         Ok(ast) => println!("File {} parsed successfully", i),
//!         Err(e) => eprintln!("File {} failed: {}", i, e),
//!     }
//! }
//! ```

use super::arena::AstArena;
use super::ast::{AstNode, ParseError};
use super::grammar::Grammar;
use super::parser::PortableParser;

#[cfg(feature = "rayon")]
use rayon::prelude::*;

/// Parse multiple inputs in parallel
///
/// # Arguments
/// * `grammar` - The grammar to use for all inputs
/// * `inputs` - Slice of input strings
///
/// # Returns
/// Vector of results in the same order as inputs
///
/// # Performance
/// - Uses rayon for work-stealing parallelism when `parallel` feature is enabled
/// - Linear speedup up to number of CPU cores
/// - Each parse has its own arena (no contention)
///
/// # Example
///
/// ```rust,ignore
/// let results = parse_batch_parallel(&grammar, &["input1", "input2", "input3"]);
/// assert_eq!(results.len(), 3);
/// ```
///
#[cfg(feature = "rayon")]
pub fn parse_batch_parallel(
    grammar: &Grammar,
    inputs: &[&str],
) -> Vec<Result<AstNode, ParseError>> {
    inputs
        .par_iter()
        .map(|input| {
            let mut arena = AstArena::for_input(input.len());
            let mut parser = PortableParser::new(grammar, input, &mut arena);
            parser.parse()
        })
        .collect()
}

/// Parse multiple inputs sequentially (fallback when rayon is not available)
///
/// This is used when the `parallel` feature is not enabled.
#[cfg(not(feature = "rayon"))]
pub fn parse_batch_parallel(
    grammar: &Grammar,
    inputs: &[&str],
) -> Vec<Result<AstNode, ParseError>> {
    inputs
        .iter()
        .map(|input| {
            let mut arena = AstArena::for_input(input.len());
            let mut parser = PortableParser::new(grammar, input, &mut arena);
            parser.parse()
        })
        .collect()
}

/// Parse multiple owned inputs in parallel
///
/// This version takes owned strings, which is useful when you have
/// a vector of Strings rather than string slices.
///
/// # Arguments
/// * `grammar` - The grammar to use for all inputs
/// * `inputs` - Vector of input strings
///
/// # Returns
/// Vector of results in the same order as inputs
///
#[cfg(feature = "rayon")]
pub fn parse_batch_parallel_owned(
    grammar: &Grammar,
    inputs: Vec<String>,
) -> Vec<Result<AstNode, ParseError>> {
    inputs
        .into_par_iter()
        .map(|input| {
            let mut arena = AstArena::for_input(input.len());
            let mut parser = PortableParser::new(grammar, &input, &mut arena);
            parser.parse()
        })
        .collect()
}

/// Parse multiple owned inputs sequentially (fallback)
#[cfg(not(feature = "rayon"))]
pub fn parse_batch_parallel_owned(
    grammar: &Grammar,
    inputs: Vec<String>,
) -> Vec<Result<AstNode, ParseError>> {
    inputs
        .into_iter()
        .map(|input| {
            let mut arena = AstArena::for_input(input.len());
            let mut parser = PortableParser::new(grammar, &input, &mut arena);
            parser.parse()
        })
        .collect()
}

/// Configuration for parallel parsing
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Number of threads to use (None = auto)
    pub num_threads: Option<usize>,
    /// Minimum chunk size for parallel processing
    pub min_chunk_size: usize,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            num_threads: None,
            min_chunk_size: 1,
        }
    }
}

impl ParallelConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the number of threads to use
    pub fn with_num_threads(mut self, n: usize) -> Self {
        self.num_threads = Some(n);
        self
    }

    /// Set the minimum chunk size for parallel processing
    pub fn with_min_chunk_size(mut self, size: usize) -> Self {
        self.min_chunk_size = size;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::super::parser_dsl::{str, GrammarBuilder};
    use super::*;

    #[test]
    fn test_parse_batch_sequential() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let inputs = vec!["hello", "hello", "hello"];
        let results = parse_batch_parallel(&grammar, &inputs);

        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_parse_batch_with_failures() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let inputs = vec!["hello", "world", "hello"];
        let results = parse_batch_parallel(&grammar, &inputs);

        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert!(results[2].is_ok());
    }

    #[test]
    fn test_parse_batch_owned() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let inputs = vec!["hello".to_string(), "hello".to_string()];
        let results = parse_batch_parallel_owned(&grammar, inputs);

        assert_eq!(results.len(), 2);
        for result in results {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_parallel_config_default() {
        let config = ParallelConfig::default();
        assert!(config.num_threads.is_none());
        assert_eq!(config.min_chunk_size, 1);
    }

    #[test]
    fn test_parallel_config_builder() {
        let config = ParallelConfig::new()
            .with_num_threads(4)
            .with_min_chunk_size(10);

        assert_eq!(config.num_threads, Some(4));
        assert_eq!(config.min_chunk_size, 10);
    }
}
