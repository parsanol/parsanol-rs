//! Parser Configuration
//!
//! Configuration types and constants for the parser.

/// Default maximum input size: 100 MB
pub const DEFAULT_MAX_INPUT_SIZE: usize = 100 * 1024 * 1024;

/// Default maximum recursion depth
pub const DEFAULT_MAX_RECURSION_DEPTH: usize = 1000;

/// Default timeout in milliseconds (0 = no timeout)
pub const DEFAULT_TIMEOUT_MS: u64 = 0;

/// Default maximum memory usage in bytes (0 = no limit)
pub const DEFAULT_MAX_MEMORY: usize = 0;

/// Check interval for timeout (number of parse operations between checks)
pub const TIMEOUT_CHECK_INTERVAL: usize = 1000;

/// Parser configuration
///
/// Holds all configurable parameters for parsing operations.
#[derive(Debug, Clone, Copy)]
pub struct ParserConfig {
    /// Maximum allowed input size in bytes
    pub max_input_size: usize,

    /// Maximum allowed recursion depth
    pub max_recursion_depth: usize,

    /// Timeout in milliseconds (0 = no timeout)
    pub timeout_ms: u64,

    /// Maximum memory usage in bytes (0 = no limit)
    pub max_memory: usize,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            max_input_size: DEFAULT_MAX_INPUT_SIZE,
            max_recursion_depth: DEFAULT_MAX_RECURSION_DEPTH,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_memory: DEFAULT_MAX_MEMORY,
        }
    }
}

impl ParserConfig {
    /// Create a new config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum input size
    pub fn with_max_input_size(mut self, size: usize) -> Self {
        self.max_input_size = size;
        self
    }

    /// Set the maximum recursion depth
    pub fn with_max_recursion_depth(mut self, depth: usize) -> Self {
        self.max_recursion_depth = depth;
        self
    }

    /// Set the timeout in milliseconds
    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    /// Set the maximum memory usage
    pub fn with_max_memory(mut self, bytes: usize) -> Self {
        self.max_memory = bytes;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ParserConfig::default();
        assert_eq!(config.max_input_size, DEFAULT_MAX_INPUT_SIZE);
        assert_eq!(config.max_recursion_depth, DEFAULT_MAX_RECURSION_DEPTH);
        assert_eq!(config.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(config.max_memory, DEFAULT_MAX_MEMORY);
    }

    #[test]
    fn test_config_builder() {
        let config = ParserConfig::new()
            .with_max_input_size(1000)
            .with_max_recursion_depth(100)
            .with_timeout_ms(5000)
            .with_max_memory(10000);

        assert_eq!(config.max_input_size, 1000);
        assert_eq!(config.max_recursion_depth, 100);
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.max_memory, 10000);
    }
}
