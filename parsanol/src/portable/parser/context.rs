//! Parse Context
//!
//! Mutable state during parsing.

use crate::portable::arena::AstArena;
use crate::portable::ast::{AstNode, ParseError};
use crate::portable::cache::DenseCache;

use super::config::TIMEOUT_CHECK_INTERVAL;

/// Mutable parsing context
///
/// Holds all mutable state during parsing, separate from the
/// immutable parser configuration.
pub struct ParseContext<'a> {
    /// AST arena for allocating nodes
    pub arena: &'a mut AstArena,

    /// Packrat memoization cache
    pub cache: DenseCache,

    /// Cached AST nodes for cache hits (stored separately to avoid lifetime issues)
    pub cached_nodes: Vec<AstNode>,

    /// Current recursion depth (tracked during parsing)
    pub current_depth: usize,

    /// Start time for timeout checking (instant::now() when parsing starts)
    pub start_time: Option<std::time::Instant>,

    /// Operation counter for periodic timeout checks
    pub op_count: usize,
}

impl<'a> ParseContext<'a> {
    /// Create a new parse context
    pub fn new(arena: &'a mut AstArena, input_len: usize, atom_count: usize) -> Self {
        let cache = DenseCache::for_input(input_len, atom_count);
        let estimated_cache_entries = (input_len / 10).clamp(64, 10000);

        Self {
            arena,
            cache,
            cached_nodes: Vec::with_capacity(estimated_cache_entries),
            current_depth: 0,
            start_time: None,
            op_count: 0,
        }
    }

    /// Create a context with pre-existing cache (for incremental parsing)
    pub fn with_cache(
        arena: &'a mut AstArena,
        cache: DenseCache,
        cached_nodes: Vec<AstNode>,
    ) -> Self {
        Self {
            arena,
            cache,
            cached_nodes,
            current_depth: 0,
            start_time: None,
            op_count: 0,
        }
    }

    /// Reset context for parsing new input
    pub fn reset(&mut self, input_len: usize, atom_count: usize) {
        self.cache = DenseCache::for_input(input_len, atom_count);
        self.cached_nodes.clear();
        self.current_depth = 0;
        self.start_time = None;
        self.op_count = 0;
    }

    /// Extract the cache and cached nodes from this context
    pub fn into_cache(self) -> (DenseCache, Vec<AstNode>) {
        (self.cache, self.cached_nodes)
    }

    /// Get current memory usage estimate
    #[inline]
    pub fn memory_usage(&self) -> usize {
        self.arena.memory_usage() + self.cache.memory_usage()
    }

    /// Store a cached node and return its index
    #[inline(always)]
    pub fn store_cached_node(&mut self, node: AstNode) -> u32 {
        let idx = self.cached_nodes.len() as u32;
        self.cached_nodes.push(node);
        idx
    }

    /// Get a cached node by index
    #[inline]
    pub fn get_cached_node(&self, idx: u32) -> AstNode {
        self.cached_nodes[idx as usize]
    }

    /// Enter a recursive call, incrementing depth counter
    #[inline]
    pub fn enter_recursive(&mut self) {
        self.current_depth += 1;
    }

    /// Exit a recursive call, decrementing depth counter
    #[inline]
    pub fn exit_recursive(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    /// Check if recursion depth exceeds limit
    #[inline]
    pub fn check_recursion_limit(&self, max_depth: usize) -> Result<(), ParseError> {
        if max_depth > 0 && self.current_depth > max_depth {
            return Err(ParseError::RecursionLimitExceeded {
                depth: self.current_depth,
                max_depth,
            });
        }
        Ok(())
    }

    /// Start the timeout timer
    #[inline]
    pub fn start_timeout_timer(&mut self) {
        self.start_time = Some(std::time::Instant::now());
        self.op_count = 0;
    }

    /// Check if timeout has been exceeded (call periodically)
    #[inline]
    pub fn check_timeout(&mut self, timeout_ms: u64) -> Result<(), ParseError> {
        if timeout_ms == 0 {
            return Ok(());
        }

        self.op_count += 1;
        if self.op_count % TIMEOUT_CHECK_INTERVAL != 0 {
            return Ok(());
        }

        if let Some(start) = self.start_time {
            let elapsed = start.elapsed().as_millis() as u64;
            if elapsed > timeout_ms {
                return Err(ParseError::TimeoutExceeded {
                    elapsed_ms: elapsed,
                    timeout_ms,
                });
            }
        }
        Ok(())
    }

    /// Check if memory usage exceeds limit
    #[inline]
    pub fn check_memory_limit(&self, max_memory: usize) -> Result<(), ParseError> {
        if max_memory > 0 && self.memory_usage() > max_memory {
            return Err(ParseError::MemoryLimitExceeded {
                used_bytes: self.memory_usage(),
                max_bytes: max_memory,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_new() {
        let mut arena = AstArena::new();
        let ctx = ParseContext::new(&mut arena, 100, 10);
        assert_eq!(ctx.current_depth, 0);
        assert!(ctx.start_time.is_none());
    }

    #[test]
    fn test_context_recursion() {
        let mut arena = AstArena::new();
        let mut ctx = ParseContext::new(&mut arena, 100, 10);

        ctx.enter_recursive();
        assert_eq!(ctx.current_depth, 1);

        ctx.exit_recursive();
        assert_eq!(ctx.current_depth, 0);
    }

    #[test]
    fn test_context_recursion_limit() {
        let mut arena = AstArena::new();
        let mut ctx = ParseContext::new(&mut arena, 100, 10);

        ctx.enter_recursive();
        ctx.enter_recursive();

        assert!(ctx.check_recursion_limit(5).is_ok());
        assert!(ctx.check_recursion_limit(1).is_err());
    }
}
