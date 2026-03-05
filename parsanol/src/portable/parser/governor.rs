//! Resource Governor for Parsing
//!
//! This module provides resource management for parsing operations,
//! enforcing limits on recursion depth, timeout, memory, and input size.
//!
//! # Architecture
//!
//! The ResourceGovernor follows the Single Responsibility Principle - its only
//! job is to track and enforce resource limits during parsing. This allows the
//! parser to focus on parsing logic while delegating resource management.
//!
//! # Example
//!
//! ```rust,ignore
//! use parsanol::portable::parser::ResourceGovernor;
//!
//! let mut governor = ResourceGovernor::new()
//!     .with_max_recursion_depth(1000)
//!     .with_timeout_ms(5000)
//!     .with_max_memory(100_000_000);
//!
//! // Enter recursive parsing
//! governor.enter_recursive()?;
//! // ... do parsing ...
//! governor.exit_recursive();
//!
//! // Check resources periodically
//! governor.check_resources()?;
//! ```

use crate::portable::ast::ParseError;

/// Default timeout check interval (check every N operations)
const TIMEOUT_CHECK_INTERVAL: usize = 1000;

/// Resource governor for parsing operations
///
/// Manages all resource limits during parsing:
/// - Recursion depth tracking
/// - Timeout enforcement
/// - Memory limit checking
/// - Input size validation
#[derive(Debug)]
pub struct ResourceGovernor {
    /// Maximum allowed input size in bytes (0 = unlimited)
    max_input_size: usize,

    /// Maximum recursion depth (0 = unlimited)
    max_recursion_depth: usize,

    /// Current recursion depth
    current_depth: usize,

    /// Timeout in milliseconds (0 = no timeout)
    timeout_ms: u64,

    /// Start time for timeout checking
    start_time: Option<std::time::Instant>,

    /// Operation counter for periodic timeout checks
    op_count: usize,

    /// Maximum memory usage in bytes (0 = unlimited)
    max_memory: usize,
}

impl Default for ResourceGovernor {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceGovernor {
    /// Create a new resource governor with no limits
    #[inline]
    pub fn new() -> Self {
        Self {
            max_input_size: 0,
            max_recursion_depth: 0,
            current_depth: 0,
            timeout_ms: 0,
            start_time: None,
            op_count: 0,
            max_memory: 0,
        }
    }

    // ========================================================================
    // Builder Methods
    // ========================================================================

    /// Set maximum input size
    #[inline]
    pub fn with_max_input_size(mut self, size: usize) -> Self {
        self.max_input_size = size;
        self
    }

    /// Set maximum recursion depth
    #[inline]
    pub fn with_max_recursion_depth(mut self, depth: usize) -> Self {
        self.max_recursion_depth = depth;
        self
    }

    /// Set timeout in milliseconds
    #[inline]
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set maximum memory usage
    #[inline]
    pub fn with_max_memory(mut self, max_memory: usize) -> Self {
        self.max_memory = max_memory;
        self
    }

    // ========================================================================
    // Configuration Getters/Setters
    // ========================================================================

    /// Get maximum input size
    #[inline]
    pub fn max_input_size(&self) -> usize {
        self.max_input_size
    }

    /// Set maximum input size
    #[inline]
    pub fn set_max_input_size(&mut self, size: usize) {
        self.max_input_size = size;
    }

    /// Get maximum recursion depth
    #[inline]
    pub fn max_recursion_depth(&self) -> usize {
        self.max_recursion_depth
    }

    /// Set maximum recursion depth
    #[inline]
    pub fn set_max_recursion_depth(&mut self, depth: usize) {
        self.max_recursion_depth = depth;
    }

    /// Get timeout in milliseconds
    #[inline]
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Set timeout in milliseconds
    #[inline]
    pub fn set_timeout_ms(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    /// Get maximum memory
    #[inline]
    pub fn max_memory(&self) -> usize {
        self.max_memory
    }

    /// Set maximum memory
    #[inline]
    pub fn set_max_memory(&mut self, max_memory: usize) {
        self.max_memory = max_memory;
    }

    /// Get current recursion depth
    #[inline]
    pub fn current_depth(&self) -> usize {
        self.current_depth
    }

    // ========================================================================
    // Resource Checking
    // ========================================================================

    /// Check input size against limit
    #[inline]
    pub fn check_input_size(&self, input_len: usize) -> Result<(), ParseError> {
        if self.max_input_size > 0 && input_len > self.max_input_size {
            return Err(ParseError::InputTooLarge {
                input_size: input_len,
                max_size: self.max_input_size,
            });
        }
        Ok(())
    }

    /// Enter a recursive call, incrementing depth counter
    #[inline]
    pub fn enter_recursive(&mut self) -> Result<(), ParseError> {
        self.current_depth += 1;
        if self.max_recursion_depth > 0 && self.current_depth > self.max_recursion_depth {
            return Err(ParseError::RecursionLimitExceeded {
                depth: self.current_depth,
                max_depth: self.max_recursion_depth,
            });
        }
        Ok(())
    }

    /// Exit a recursive call, decrementing depth counter
    #[inline]
    pub fn exit_recursive(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    /// Start the timeout timer
    #[inline]
    pub fn start_timeout_timer(&mut self) {
        if self.timeout_ms > 0 {
            self.start_time = Some(std::time::Instant::now());
            self.op_count = 0;
        }
    }

    /// Check if timeout has been exceeded
    #[inline]
    pub fn check_timeout(&mut self) -> Result<(), ParseError> {
        if self.timeout_ms == 0 {
            return Ok(());
        }

        self.op_count += 1;
        if self.op_count % TIMEOUT_CHECK_INTERVAL != 0 {
            return Ok(());
        }

        if let Some(start) = self.start_time {
            let elapsed = start.elapsed().as_millis() as u64;
            if elapsed > self.timeout_ms {
                return Err(ParseError::TimeoutExceeded {
                    elapsed_ms: elapsed,
                    timeout_ms: self.timeout_ms,
                });
            }
        }
        Ok(())
    }

    /// Check if memory usage exceeds limit
    #[inline]
    pub fn check_memory(&self, current_usage: usize) -> Result<(), ParseError> {
        if self.max_memory > 0 && current_usage > self.max_memory {
            return Err(ParseError::MemoryLimitExceeded {
                used_bytes: current_usage,
                max_bytes: self.max_memory,
            });
        }
        Ok(())
    }

    /// Check all resources (timeout and memory)
    ///
    /// This should be called periodically during parsing.
    /// Memory is only checked at the same interval as timeout.
    #[inline]
    pub fn check_resources(&mut self, current_memory_usage: usize) -> Result<(), ParseError> {
        self.check_timeout()?;
        if self.max_memory > 0 && self.op_count % TIMEOUT_CHECK_INTERVAL == 0 {
            self.check_memory(current_memory_usage)?;
        }
        Ok(())
    }

    /// Reset state for a new parsing operation
    #[inline]
    pub fn reset(&mut self) {
        self.current_depth = 0;
        self.start_time = None;
        self.op_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governor_defaults() {
        let governor = ResourceGovernor::new();
        assert_eq!(governor.max_input_size(), 0);
        assert_eq!(governor.max_recursion_depth(), 0);
        assert_eq!(governor.timeout_ms(), 0);
        assert_eq!(governor.max_memory(), 0);
    }

    #[test]
    fn test_governor_builder() {
        let governor = ResourceGovernor::new()
            .with_max_input_size(1000)
            .with_max_recursion_depth(100)
            .with_timeout_ms(5000)
            .with_max_memory(1_000_000);

        assert_eq!(governor.max_input_size(), 1000);
        assert_eq!(governor.max_recursion_depth(), 100);
        assert_eq!(governor.timeout_ms(), 5000);
        assert_eq!(governor.max_memory(), 1_000_000);
    }

    #[test]
    fn test_recursion_tracking() {
        let mut governor = ResourceGovernor::new().with_max_recursion_depth(3);

        assert!(governor.enter_recursive().is_ok()); // depth = 1
        assert!(governor.enter_recursive().is_ok()); // depth = 2
        assert!(governor.enter_recursive().is_ok()); // depth = 3
        assert!(governor.enter_recursive().is_err()); // depth = 4, exceeds limit

        governor.exit_recursive(); // depth = 3
                                   // After exit, depth = 3, entering again makes depth = 4 which still exceeds limit
        assert!(governor.enter_recursive().is_err()); // depth = 4, exceeds limit
    }

    #[test]
    fn test_input_size_check() {
        let governor = ResourceGovernor::new().with_max_input_size(100);

        assert!(governor.check_input_size(50).is_ok());
        assert!(governor.check_input_size(100).is_ok());
        assert!(governor.check_input_size(101).is_err());
    }

    #[test]
    fn test_memory_check() {
        let governor = ResourceGovernor::new().with_max_memory(1000);

        assert!(governor.check_memory(500).is_ok());
        assert!(governor.check_memory(1000).is_ok());
        assert!(governor.check_memory(1001).is_err());
    }

    #[test]
    fn test_reset() {
        let mut governor = ResourceGovernor::new();
        governor.enter_recursive().ok();
        governor.start_timeout_timer();

        governor.reset();

        assert_eq!(governor.current_depth(), 0);
        assert!(governor.start_time.is_none());
    }
}
