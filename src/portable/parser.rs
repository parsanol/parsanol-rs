//! Portable PEG Parser
//!
//! This module implements the core parsing engine that can be used
//! standalone (for WASM) or integrated with Ruby FFI.

use super::{
    arena::AstArena,
    ast::{AstNode, ParseError, ParseResult},
    cache::DenseCache,
    char_class::{utf8_char_len, CharacterPattern},
    grammar::Grammar,
    regex_cache,
};

/// Logging macros - no-op when logging feature is disabled
#[cfg(not(feature = "logging"))]
macro_rules! log_debug {
    ($($arg:tt)*) => {};
}

/// Logging macros - use log crate when logging feature is enabled
#[cfg(feature = "logging")]
macro_rules! log_debug {
    ($($arg:tt)*) => { log::debug!($($arg)*) };
}

/// SIMD-optimized functions for bulk character operations
mod simd_helpers {
    /// Skip all characters matching a predicate, returning the new position
    /// Uses SIMD when available via memchr for common patterns
    #[inline]
    pub fn skip_while<F: Fn(u8) -> bool>(input: &[u8], pos: usize, predicate: F) -> usize {
        let mut current = pos;
        let len = input.len();

        // Process in chunks for better cache locality
        while current + 8 <= len {
            // Process 8 bytes at a time
            let chunk = &input[current..current + 8];
            let mut stop = false;
            for &b in chunk {
                if !predicate(b) {
                    stop = true;
                    break;
                }
                current += 1;
            }
            if stop {
                return current;
            }
        }

        // Handle remaining bytes
        while current < len && predicate(input[current]) {
            current += 1;
        }

        current
    }
}

/// Default maximum input size: 100 MB
pub const DEFAULT_MAX_INPUT_SIZE: usize = 100 * 1024 * 1024;

/// Default maximum recursion depth
pub const DEFAULT_MAX_RECURSION_DEPTH: usize = 1000;

/// Default timeout in milliseconds (0 = no timeout)
pub const DEFAULT_TIMEOUT_MS: u64 = 0;

/// Default maximum memory usage in bytes (0 = no limit)
pub const DEFAULT_MAX_MEMORY: usize = 0;

/// Check interval for timeout (number of parse operations between checks)
const TIMEOUT_CHECK_INTERVAL: usize = 1000;

/// Mutable parsing context
///
/// This struct holds the mutable state during parsing, separate from the
/// immutable parser configuration. This separation enables:
/// - Cache reuse across multiple parses with the same grammar
/// - Incremental parsing with preserved state
/// - Easier testing of individual components
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::portable::{Grammar, AstArena, ParseContext};
///
/// let grammar = Grammar::new(/* ... */);
/// let input = "hello world";
/// let mut arena = AstArena::for_input(input.len());
/// let mut context = ParseContext::new(&mut arena, input.len(), grammar.atom_count());
///
/// // Parse using the context
/// // parser.parse_with_context(&mut context)
///
/// // Reuse context for incremental parsing
/// context.reset_for_input(new_input.len());
/// ```
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
    ///
    /// # Arguments
    /// * `arena` - Mutable reference to the AST arena
    /// * `input_len` - Length of input (for cache sizing)
    /// * `atom_count` - Number of atoms in grammar (for cache sizing)
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
    ///
    /// # Arguments
    /// * `arena` - Mutable reference to the AST arena
    /// * `cache` - Pre-existing cache to reuse
    /// * `cached_nodes` - Pre-existing cached nodes to reuse
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
    ///
    /// Clears the cache and cached nodes, preparing for a new parse.
    /// The arena is not cleared by default (use `arena.reset()` separately if needed).
    pub fn reset(&mut self, input_len: usize, atom_count: usize) {
        self.cache = DenseCache::for_input(input_len, atom_count);
        self.cached_nodes.clear();
        self.current_depth = 0;
        self.start_time = None;
        self.op_count = 0;
    }

    /// Extract the cache and cached nodes from this context
    ///
    /// Useful for incremental parsing where you want to preserve the cache
    /// between parses.
    pub fn into_cache(self) -> (DenseCache, Vec<AstNode>) {
        (self.cache, self.cached_nodes)
    }

    /// Get current memory usage estimate
    #[inline]
    pub fn memory_usage(&self) -> usize {
        self.arena.memory_usage() + self.cache.memory_usage()
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

/// Configuration options for the parser
///
/// This struct bundles all configurable parameters for parsing operations.
/// Use [`ParserConfig::default()`] for sensible defaults, or customize
/// individual fields as needed.
///
/// # Example
///
/// ```rust
/// use parsanol::portable::parser::ParserConfig;
///
/// let config = ParserConfig {
///     max_input_size: 10 * 1024 * 1024,  // 10 MB
///     max_recursion_depth: 500,
///     timeout_ms: 5000,  // 5 seconds
///     max_memory: 50 * 1024 * 1024,  // 50 MB
/// };
/// ```
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

/// The portable parser engine
pub struct PortableParser<'a> {
    /// The compiled grammar
    grammar: &'a Grammar,

    /// Input string (UTF-8)
    input: &'a str,

    /// Input as bytes (for fast indexing)
    input_bytes: &'a [u8],

    /// AST arena
    arena: &'a mut AstArena,

    /// Packrat cache
    cache: DenseCache,

    /// Cached AST nodes for cache hits (stored separately to avoid lifetime issues)
    cached_nodes: Vec<AstNode>,

    /// Maximum allowed input size in bytes
    max_input_size: usize,

    /// Maximum allowed recursion depth
    max_recursion_depth: usize,

    /// Current recursion depth (tracked during parsing)
    current_depth: usize,

    /// Timeout in milliseconds (0 = no timeout)
    timeout_ms: u64,

    /// Start time for timeout checking (instant::now() when parsing starts)
    start_time: Option<std::time::Instant>,

    /// Operation counter for periodic timeout checks
    op_count: usize,

    /// Maximum memory usage in bytes (0 = no limit)
    max_memory: usize,
}

impl<'a> PortableParser<'a> {
    /// Create a new parser with default security limits
    ///
    /// # Security Limits
    /// - Maximum input size: 100 MB
    /// - Maximum recursion depth: 1000
    ///
    /// Use [`with_limits`](Self::with_limits) to customize these limits.
    #[inline]
    pub fn new(grammar: &'a Grammar, input: &'a str, arena: &'a mut AstArena) -> Self {
        Self::with_limits(
            grammar,
            input,
            arena,
            DEFAULT_MAX_INPUT_SIZE,
            DEFAULT_MAX_RECURSION_DEPTH,
        )
    }

    /// Create a new parser with a pre-existing cache
    ///
    /// This is used for incremental parsing where cache entries from a previous
    /// parse should be reused.
    ///
    /// # Arguments
    /// * `grammar` - The compiled grammar
    /// * `input` - Input string to parse
    /// * `arena` - AST arena for allocation
    /// * `cache` - Pre-existing cache to use
    /// * `cached_nodes` - Pre-existing cached AST nodes (must match cache entries)
    ///
    /// # Example
    /// ```rust,ignore
    /// // First parse
    /// let mut parser = PortableParser::new(&grammar, input1, &mut arena);
    /// let result = parser.parse()?;
    /// let (cache, cached_nodes) = parser.into_cache();
    ///
    /// // Incremental re-parse with preserved cache
    /// let mut parser = PortableParser::new_with_cache(
    ///     &grammar, input2, &mut arena, cache, cached_nodes
    /// );
    /// let result = parser.parse()?;
    /// ```
    #[inline]
    pub fn new_with_cache(
        grammar: &'a Grammar,
        input: &'a str,
        arena: &'a mut AstArena,
        cache: DenseCache,
        cached_nodes: Vec<AstNode>,
    ) -> Self {
        let input_bytes = input.as_bytes();

        Self {
            grammar,
            input,
            input_bytes,
            arena,
            cache,
            cached_nodes,
            max_input_size: DEFAULT_MAX_INPUT_SIZE,
            max_recursion_depth: DEFAULT_MAX_RECURSION_DEPTH,
            current_depth: 0,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            start_time: None,
            op_count: 0,
            max_memory: DEFAULT_MAX_MEMORY,
        }
    }

    /// Extract the cache and cached nodes from this parser
    ///
    /// This is used for incremental parsing to preserve cache state across parses.
    /// After calling this, the parser should not be used again.
    ///
    /// # Returns
    /// A tuple of (cache, cached_nodes) that can be passed to `new_with_cache`.
    #[inline]
    pub fn into_cache(self) -> (DenseCache, Vec<AstNode>) {
        (self.cache, self.cached_nodes)
    }

    /// Create a new parser with custom security limits
    ///
    /// # Arguments
    /// * `grammar` - The compiled grammar
    /// * `input` - Input string to parse
    /// * `arena` - AST arena for allocation
    /// * `max_input_size` - Maximum allowed input size in bytes (0 = unlimited)
    /// * `max_recursion_depth` - Maximum allowed recursion depth (0 = unlimited)
    ///
    /// # Errors
    /// Returns `ParseError::InputTooLarge` if input exceeds `max_input_size`.
    #[inline]
    pub fn with_limits(
        grammar: &'a Grammar,
        input: &'a str,
        arena: &'a mut AstArena,
        max_input_size: usize,
        max_recursion_depth: usize,
    ) -> Self {
        let input_bytes = input.as_bytes();
        let cache = DenseCache::for_input(input.len(), grammar.atom_count());
        // Pre-allocate cached_nodes based on input size to avoid reallocations
        let estimated_cache_entries = (input.len() / 10).clamp(64, 10000);

        Self {
            grammar,
            input,
            input_bytes,
            arena,
            cache,
            cached_nodes: Vec::with_capacity(estimated_cache_entries),
            max_input_size,
            max_recursion_depth,
            current_depth: 0,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            start_time: None,
            op_count: 0,
            max_memory: DEFAULT_MAX_MEMORY,
        }
    }

    /// Set the maximum input size (0 = unlimited)
    #[inline]
    pub fn set_max_input_size(&mut self, size: usize) {
        self.max_input_size = size;
    }

    /// Set the maximum recursion depth (0 = unlimited)
    #[inline]
    pub fn set_max_recursion_depth(&mut self, depth: usize) {
        self.max_recursion_depth = depth;
    }

    /// Set the timeout in milliseconds (0 = no timeout)
    ///
    /// When set, parsing will be aborted if it takes longer than the specified time.
    /// The timeout is checked periodically during parsing to minimize overhead.
    #[inline]
    pub fn set_timeout_ms(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    /// Set the maximum memory usage in bytes (0 = unlimited)
    ///
    /// When set, parsing will be aborted if memory usage exceeds the specified limit.
    /// Memory is checked periodically during parsing.
    #[inline]
    pub fn set_max_memory(&mut self, max_memory: usize) {
        self.max_memory = max_memory;
    }

    /// Get current memory usage estimate
    #[inline]
    pub fn memory_usage(&self) -> usize {
        self.arena.memory_usage() + self.cache.memory_usage()
    }

    /// Check if input size is within limits
    #[inline]
    fn check_input_size(&self) -> Result<(), ParseError> {
        if self.max_input_size > 0 && self.input.len() > self.max_input_size {
            return Err(ParseError::InputTooLarge {
                input_size: self.input.len(),
                max_size: self.max_input_size,
            });
        }
        Ok(())
    }

    /// Enter a recursive call, checking depth limits
    #[inline]
    fn enter_recursive(&mut self) -> Result<(), ParseError> {
        self.current_depth += 1;
        if self.max_recursion_depth > 0 && self.current_depth > self.max_recursion_depth {
            return Err(ParseError::RecursionLimitExceeded {
                depth: self.current_depth,
                max_depth: self.max_recursion_depth,
            });
        }
        Ok(())
    }

    /// Exit a recursive call
    #[inline]
    fn exit_recursive(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    /// Start the timeout timer
    #[inline]
    fn start_timeout_timer(&mut self) {
        if self.timeout_ms > 0 {
            self.start_time = Some(std::time::Instant::now());
            self.op_count = 0;
        }
    }

    /// Check if timeout has been exceeded
    /// This is called periodically to minimize overhead
    #[inline]
    fn check_timeout(&mut self) -> Result<(), ParseError> {
        if self.timeout_ms == 0 {
            return Ok(());
        }

        self.op_count += 1;

        // Only check time every TIMEOUT_CHECK_INTERVAL operations
        if self.op_count % TIMEOUT_CHECK_INTERVAL != 0 {
            return Ok(());
        }

        if let Some(start) = self.start_time {
            let elapsed = start.elapsed();
            let elapsed_ms = elapsed.as_millis() as u64;
            if elapsed_ms > self.timeout_ms {
                return Err(ParseError::TimeoutExceeded {
                    elapsed_ms,
                    timeout_ms: self.timeout_ms,
                });
            }
        }

        Ok(())
    }

    /// Check if memory usage is within limits
    #[inline]
    fn check_memory(&self) -> Result<(), ParseError> {
        if self.max_memory == 0 {
            return Ok(());
        }

        let used = self.memory_usage();
        if used > self.max_memory {
            return Err(ParseError::MemoryLimitExceeded {
                used_bytes: used,
                max_bytes: self.max_memory,
            });
        }

        Ok(())
    }

    /// Combined resource check (timeout + memory)
    /// Call this periodically during parsing
    #[inline]
    fn check_resources(&mut self) -> Result<(), ParseError> {
        self.check_timeout()?;
        if self.max_memory > 0 && self.op_count % TIMEOUT_CHECK_INTERVAL == 0 {
            self.check_memory()?;
        }
        Ok(())
    }

    /// Parse the input and return the result
    ///
    /// # Errors
    /// Returns an error if:
    /// - Input exceeds the maximum size limit
    /// - Recursion depth exceeds the limit
    /// - Parsing fails
    #[inline]
    pub fn parse(&mut self) -> Result<AstNode, ParseError> {
        // Check input size limit first
        self.check_input_size()?;

        // Start timeout timer
        self.start_timeout_timer();

        log_debug!(
            "Starting parse: input_len={}, root_atom={}",
            self.input.len(),
            self.grammar.root
        );

        match self.try_atom(self.grammar.root, 0) {
            Ok(result) => {
                if result.end_pos == self.input.len() {
                    log_debug!("Parse successful: consumed all input");
                    Ok(result.value)
                } else {
                    // Cold path - incomplete parse
                    Err(ParseError::Incomplete {
                        expected: self.input.len(),
                        actual: result.end_pos,
                    })
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Parse with custom configuration
    ///
    /// This method applies the configuration from `config` before parsing.
    /// Note that some config options (like `max_input_size`) are checked at
    /// parser creation time, so this method is primarily useful for
    /// adjusting limits before parsing.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = ParserConfig::new()
    ///     .with_max_input_size(10 * 1024 * 1024)
    ///     .with_timeout_ms(5000);
    ///
    /// let result = parser.parse_with_config(config);
    /// ```
    pub fn parse_with_config(&mut self, config: ParserConfig) -> Result<AstNode, ParseError> {
        // Apply configuration
        self.max_input_size = config.max_input_size;
        self.max_recursion_depth = config.max_recursion_depth;
        self.timeout_ms = config.timeout_ms;
        self.max_memory = config.max_memory;

        // Parse with the applied configuration
        self.parse()
    }

    /// Parse input with a streaming builder
    ///
    /// This method parses the input and streams events to the builder
    /// during parsing, eliminating the intermediate AST step for the consumer.
    ///
    /// Note: The current implementation first builds the AST internally, then
    /// walks it to send events to the builder. A future optimization will
    /// stream events directly during parsing for true single-pass operation.
    ///
    /// # Advantages
    /// - **Flexible output**: Builder can produce any output type
    /// - **Memory efficient**: Builder decides what to keep
    /// - **Type-safe**: Builder's Output type is the return type
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use parsanol::portable::{PortableParser, streaming_builder::{StreamingBuilder, BuildResult}};
    ///
    /// // Custom builder that collects strings
    /// struct StringCollector {
    ///     result: Vec<String>,
    /// }
    ///
    /// impl StreamingBuilder for StringCollector {
    ///     type Output = Vec<String>;
    ///
    ///     fn on_string(&mut self, value: &str, _: usize, _: usize) -> BuildResult<()> {
    ///         self.result.push(value.to_string());
    ///         Ok(())
    ///     }
    ///
    ///     fn finish(&mut self) -> BuildResult<Vec<String>> {
    ///         Ok(std::mem::take(&mut self.result))
    ///     }
    /// }
    ///
    /// let mut builder = StringCollector { result: vec![] };
    /// let result = parser.parse_with_builder(&mut builder)?;
    /// ```
    ///
    pub fn parse_with_builder<B: super::streaming_builder::StreamingBuilder>(
        &mut self,
        builder: &mut B,
    ) -> Result<B::Output, ParseError> {
        use super::streaming_builder::walk_ast;

        // Initialize builder
        builder
            .on_start(self.input)
            .map_err(|e| ParseError::BuilderError {
                message: e.to_string(),
            })?;

        // Parse to get the AST
        let ast = self.parse()?;

        // Walk the AST and send events to the builder
        walk_ast(&ast, self.arena, self.input, builder).map_err(|e| ParseError::BuilderError {
            message: e.to_string(),
        })?;

        // Finalize
        builder.on_success().map_err(|e| ParseError::BuilderError {
            message: e.to_string(),
        })?;

        builder.finish().map_err(|e| ParseError::BuilderError {
            message: e.to_string(),
        })
    }

    /// Store a cached AST node and return its reference
    #[inline(always)]
    fn store_cached_node(&mut self, node: AstNode) -> u32 {
        let idx = self.cached_nodes.len() as u32;
        self.cached_nodes.push(node);
        idx
    }

    /// Try to match an atom at the given position
    #[inline]
    fn try_atom(&mut self, atom_id: usize, pos: usize) -> Result<ParseResult, ParseError> {
        // Check resources periodically (timeout + memory)
        self.check_resources()?;

        // Check cache first - extract values before using
        let cache_hit = self
            .cache
            .get(pos as u32, atom_id as u16)
            .map(|e| (e.success(), e.end_pos, e.ast_ref()));

        if let Some((success, end_pos, ast_ref)) = cache_hit {
            return if success {
                // Return the cached AST node
                let cached_node = self.cached_nodes[ast_ref as usize];
                Ok(ParseResult {
                    value: cached_node,
                    end_pos: end_pos as usize,
                })
            } else {
                Err(ParseError::Failed { position: pos })
            };
        }

        // Parse the atom
        let result = self.parse_atom_uncached(atom_id, pos)?;

        // Store the AST node in cache
        let ast_ref = self.store_cached_node(result.value);

        // Cache the result
        self.cache.insert(super::cache::CacheEntry::new(
            pos as u32,
            atom_id as u16,
            true,
            result.end_pos as u32,
            ast_ref,
        ));

        // Return a copy of the result
        Ok(ParseResult {
            value: self.cached_nodes[ast_ref as usize],
            end_pos: result.end_pos,
        })
    }

    /// Parse an atom without checking the cache
    #[inline]
    fn parse_atom_uncached(
        &mut self,
        atom_id: usize,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        match self.grammar.get_atom(atom_id) {
            Some(atom) => match atom {
                super::grammar::Atom::Str { pattern } => self.parse_str(pattern, pos),
                super::grammar::Atom::Re { pattern } => self.parse_re(pattern, pos),
                super::grammar::Atom::Sequence { atoms } => self.parse_sequence(atoms, pos),
                super::grammar::Atom::Alternative { atoms } => self.parse_alternative(atoms, pos),
                super::grammar::Atom::Repetition { atom, min, max } => {
                    self.parse_repetition(*atom, *min, *max, pos)
                }
                super::grammar::Atom::Named { name, atom } => self.parse_named(name, *atom, pos),
                super::grammar::Atom::Entity { atom } => {
                    // Track recursion depth for entity references
                    self.enter_recursive()?;
                    let result = self.try_atom(*atom, pos);
                    self.exit_recursive();
                    result
                }
                super::grammar::Atom::Lookahead { atom, positive } => {
                    self.parse_lookahead(*atom, *positive, pos)
                }
                super::grammar::Atom::Cut => Ok(ParseResult {
                    value: AstNode::Nil,
                    end_pos: pos,
                }),
                super::grammar::Atom::Ignore { atom } => {
                    // Match the inner atom but discard the result
                    let result = self.try_atom(*atom, pos)?;
                    Ok(ParseResult {
                        value: AstNode::Nil,
                        end_pos: result.end_pos,
                    })
                }
            },
            None => Err(ParseError::Internal {
                message: "Invalid atom ID".to_string(),
            }),
        }
    }

    /// Parse a literal string
    #[inline]
    fn parse_str(&mut self, pattern: &str, pos: usize) -> Result<ParseResult, ParseError> {
        let pattern_bytes = pattern.as_bytes();
        let pattern_len = pattern_bytes.len();
        let end = pos + pattern_len;

        if end > self.input.len() {
            return Err(ParseError::Failed { position: pos });
        }

        let slice = &self.input_bytes[pos..end];
        if slice == pattern_bytes {
            Ok(ParseResult {
                value: self.arena.input_ref(pos, pattern_len),
                end_pos: end,
            })
        } else {
            Err(ParseError::Failed { position: pos })
        }
    }

    /// Parse a regular expression pattern
    #[inline]
    fn parse_re(&mut self, pattern: &str, pos: usize) -> Result<ParseResult, ParseError> {
        if pos >= self.input.len() {
            return Err(ParseError::Failed { position: pos });
        }

        let b = self.input_bytes[pos];

        // Fast paths for common patterns using CharacterPattern enum (O(1) lookup)
        if let Some(char_pattern) = CharacterPattern::from_pattern(pattern) {
            if char_pattern.matches(b) {
                // Determine character length based on pattern type
                let char_len = match char_pattern {
                    CharacterPattern::Any
                    | CharacterPattern::NonDigit
                    | CharacterPattern::NonSpace
                    | CharacterPattern::NonWord => {
                        // These patterns can match multi-byte UTF-8
                        utf8_char_len(b)
                    }
                    _ => {
                        // Single-byte patterns (ASCII)
                        1
                    }
                };
                return Ok(ParseResult {
                    value: self.arena.input_ref(pos, char_len),
                    end_pos: pos + char_len,
                });
            } else {
                return Err(ParseError::Failed { position: pos });
            }
        }

        // General case: use regex crate with caching
        let regex = match regex_cache::get_or_compile(pattern) {
            Some(r) => r,
            None => {
                return Err(ParseError::Internal {
                    message: format!("Invalid regex pattern: {}", pattern),
                });
            }
        };

        let remaining = &self.input[pos..];
        if let Some(m) = regex.find(remaining) {
            if m.start() == 0 {
                let match_len = m.end();
                return Ok(ParseResult {
                    value: self.arena.input_ref(pos, match_len),
                    end_pos: pos + match_len,
                });
            }
        }

        Err(ParseError::Failed { position: pos })
    }

    /// Parse a sequence of atoms
    #[inline]
    fn parse_sequence(&mut self, atoms: &[usize], pos: usize) -> Result<ParseResult, ParseError> {
        let mut current_pos = pos;
        // Pre-allocate with exact capacity to avoid reallocations
        let mut items = Vec::with_capacity(atoms.len());

        for &atom_id in atoms {
            let result = self.try_atom(atom_id, current_pos)?;
            items.push(result.value);
            current_pos = result.end_pos;
        }

        let (pool_idx, len) = self.arena.store_array(&items);
        Ok(ParseResult {
            value: AstNode::Array {
                pool_index: pool_idx,
                length: len,
            },
            end_pos: current_pos,
        })
    }

    /// Parse alternatives (ordered choice)
    #[inline]
    fn parse_alternative(
        &mut self,
        atoms: &[usize],
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        for &atom_id in atoms {
            if let Ok(result) = self.try_atom(atom_id, pos) {
                return Ok(result);
            }
        }
        Err(ParseError::Failed { position: pos })
    }

    /// Parse repetition (greedy, with min/max)
    /// Optimized with capacity hints and loop unrolling for max bounds
    #[inline]
    fn parse_repetition(
        &mut self,
        atom_id: usize,
        min: usize,
        max: Option<usize>,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        // Check if we can use SIMD-optimized bulk matching for simple character classes
        if let Some(super::grammar::Atom::Re { pattern }) = self.grammar.get_atom(atom_id) {
            // Try to get a predicate function for this pattern
            if let Some(predicate) = self.get_char_predicate(pattern) {
                return self.parse_repetition_bulk(atom_id, predicate, min, max, pos);
            }
        }

        // Fallback to standard parsing with optimized loop structure
        let mut current_pos = pos;
        let mut count = 0;
        // Pre-allocate with capacity hint - use min or 8 as baseline
        let mut items: Vec<AstNode> = Vec::with_capacity(min.clamp(8, 64));

        // Optimized loop structure - move max check outside loop when possible
        if let Some(max_count) = max {
            // Bounded loop - max check is known
            while count < max_count {
                match self.try_atom(atom_id, current_pos) {
                    Ok(result) => {
                        items.push(result.value);
                        current_pos = result.end_pos;
                        count += 1;
                    }
                    Err(_) => break,
                }
            }
        } else {
            // Unbounded loop - no max check needed
            while let Ok(result) = self.try_atom(atom_id, current_pos) {
                items.push(result.value);
                current_pos = result.end_pos;
                count += 1;
            }
        }

        if count < min {
            return Err(ParseError::Failed { position: pos });
        }

        let (pool_idx, len) = self.arena.store_array(&items);
        Ok(ParseResult {
            value: AstNode::Array {
                pool_index: pool_idx,
                length: len,
            },
            end_pos: current_pos,
        })
    }

    /// Get a predicate function for common character patterns
    /// Returns None for patterns that need full regex matching
    #[inline]
    fn get_char_predicate(&self, pattern: &str) -> Option<fn(u8) -> bool> {
        CharacterPattern::from_pattern(pattern).map(|p| p.predicate())
    }

    /// SIMD-optimized bulk repetition parsing for simple character classes
    #[inline]
    fn parse_repetition_bulk(
        &mut self,
        _atom_id: usize,
        predicate: fn(u8) -> bool,
        min: usize,
        max: Option<usize>,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        use self::simd_helpers::skip_while;

        // Skip all matching characters in one pass (SIMD-optimized)
        let end_pos = skip_while(self.input_bytes, pos, predicate);

        // Calculate count
        let count = end_pos - pos;

        // Check minimum
        if count < min {
            return Err(ParseError::Failed { position: pos });
        }

        // Apply maximum if specified
        let actual_end = if let Some(max_count) = max {
            if count > max_count {
                pos + max_count
            } else {
                end_pos
            }
        } else {
            end_pos
        };

        let actual_count = actual_end - pos;

        // For repetitions, we return a single InputRef covering the entire match
        // This is more efficient than returning an array of individual characters
        Ok(ParseResult {
            value: self.arena.input_ref(pos, actual_count),
            end_pos: actual_end,
        })
    }

    /// Parse a named capture
    #[inline]
    fn parse_named(
        &mut self,
        name: &str,
        atom_id: usize,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        let result = self.try_atom(atom_id, pos)?;

        let (pool_idx, len) = self.arena.store_hash(&[(name, result.value)]);
        Ok(ParseResult {
            value: AstNode::Hash {
                pool_index: pool_idx,
                length: len,
            },
            end_pos: result.end_pos,
        })
    }

    /// Parse lookahead (doesn't consume input)
    #[inline]
    fn parse_lookahead(
        &mut self,
        atom_id: usize,
        positive: bool,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        let matches = self.try_atom(atom_id, pos).is_ok();

        if matches == positive {
            Ok(ParseResult {
                value: AstNode::Nil,
                end_pos: pos,
            })
        } else {
            Err(ParseError::Failed { position: pos })
        }
    }

    /// Parse with rich error reporting
    ///
    /// On failure, returns a RichError with tree structure showing
    /// what was expected and what failed.
    #[allow(clippy::result_large_err)]
    pub fn parse_with_rich_error(&mut self) -> Result<AstNode, super::error::RichError> {
        use super::error::{offset_to_line_col, RichError};

        match self.try_atom_with_error(self.grammar.root, 0, None) {
            Ok(result) => {
                if result.end_pos == self.input.len() {
                    Ok(result.value)
                } else {
                    let (line, col) = offset_to_line_col(self.input, result.end_pos);
                    Err(RichError::at_position(
                        format!(
                            "Incomplete parse: consumed {} of {} bytes",
                            result.end_pos,
                            self.input.len()
                        ),
                        result.end_pos,
                        line,
                        col,
                    ))
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Try to match an atom and build rich error on failure
    #[allow(clippy::result_large_err)]
    fn try_atom_with_error(
        &mut self,
        atom_id: usize,
        pos: usize,
        context: Option<&str>,
    ) -> Result<ParseResult, super::error::RichError> {
        use super::error::{offset_to_line_col, ErrorBuilder, RichError, Span};

        // First try normal parsing
        match self.try_atom(atom_id, pos) {
            Ok(result) => Ok(result),
            Err(ParseError::Failed { position }) => {
                let (line, col) = offset_to_line_col(self.input, position);
                let span = Span::at(position, line, col);

                // Build a rich error with context
                let atom = self.grammar.get_atom(atom_id);
                let message = self.describe_atom_failure(atom, position);

                let mut error = ErrorBuilder::new(message).span(span).build();

                if let Some(ctx) = context {
                    error = error.with_context(ctx);
                }

                Err(error)
            }
            Err(ParseError::Incomplete { expected, actual }) => {
                let (line, col) = offset_to_line_col(self.input, actual);
                Err(RichError::at_position(
                    format!("Incomplete: expected {} bytes, got {}", expected, actual),
                    actual,
                    line,
                    col,
                ))
            }
            Err(ParseError::Internal { message }) => {
                let (line, col) = offset_to_line_col(self.input, pos);
                Err(RichError::at_position(
                    format!("Internal error: {}", message),
                    pos,
                    line,
                    col,
                ))
            }
            Err(ParseError::InvalidGrammar { reason }) => Err(RichError::at_position(
                format!("Invalid grammar: {}", reason),
                0,
                1,
                1,
            )),
            Err(ParseError::InputTooLarge {
                input_size,
                max_size,
            }) => Err(RichError::at_position(
                format!(
                    "Input too large: {} bytes exceeds limit of {} bytes",
                    input_size, max_size
                ),
                0,
                1,
                1,
            )),
            Err(ParseError::RecursionLimitExceeded { depth, max_depth }) => {
                let (line, col) = offset_to_line_col(self.input, pos);
                Err(RichError::at_position(
                    format!(
                        "Recursion limit exceeded: depth {} exceeds limit of {}",
                        depth, max_depth
                    ),
                    pos,
                    line,
                    col,
                ))
            }
            Err(ParseError::TimeoutExceeded {
                elapsed_ms,
                timeout_ms,
            }) => Err(RichError::at_position(
                format!(
                    "Timeout exceeded: {}ms exceeds limit of {}ms",
                    elapsed_ms, timeout_ms
                ),
                0,
                1,
                1,
            )),
            Err(ParseError::MemoryLimitExceeded {
                used_bytes,
                max_bytes,
            }) => Err(RichError::at_position(
                format!(
                    "Memory limit exceeded: {} bytes exceeds limit of {} bytes",
                    used_bytes, max_bytes
                ),
                0,
                1,
                1,
            )),
            Err(ParseError::BuilderError { message }) => {
                let (line, col) = offset_to_line_col(self.input, pos);
                Err(RichError::at_position(
                    format!("Builder error: {}", message),
                    pos,
                    line,
                    col,
                ))
            }
        }
    }

    /// Describe why an atom failed to match
    fn describe_atom_failure(&self, atom: Option<&super::grammar::Atom>, pos: usize) -> String {
        let char_at = if pos < self.input.len() {
            // Get the character at position, handling multi-byte UTF-8 safely
            match self.input[pos..].chars().next() {
                Some(c) => format!("{:?}", c),
                None => "end of input".to_string(),
            }
        } else {
            "end of input".to_string()
        };

        match atom {
            Some(super::grammar::Atom::Str { pattern }) => {
                format!("Expected {:?}, found {}", pattern, char_at)
            }
            Some(super::grammar::Atom::Re { pattern }) => {
                format!("Expected pattern {:?}, found {}", pattern, char_at)
            }
            Some(super::grammar::Atom::Sequence { atoms }) => {
                format!(
                    "Failed to match sequence of {} items at {}",
                    atoms.len(),
                    char_at
                )
            }
            Some(super::grammar::Atom::Alternative { atoms }) => {
                format!(
                    "Expected one of {} alternatives, found {}",
                    atoms.len(),
                    char_at
                )
            }
            Some(super::grammar::Atom::Repetition { min, max, .. }) => {
                let max_str = max
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "∞".to_string());
                format!("Expected {}..{} repetitions at {}", min, max_str, char_at)
            }
            Some(super::grammar::Atom::Named { name, .. }) => {
                format!("Failed to match {:?} at {}", name, char_at)
            }
            Some(super::grammar::Atom::Lookahead { positive, .. }) => {
                if *positive {
                    format!("Positive lookahead failed at {}", char_at)
                } else {
                    format!("Negative lookahead failed at {}", char_at)
                }
            }
            _ => format!("Failed to match at {}", char_at),
        }
    }

    /// Parse with tracing enabled
    ///
    /// Returns the parse result along with a trace of all parsing steps.
    pub fn parse_with_trace(&mut self) -> (Result<AstNode, ParseError>, super::debug::ParseTrace) {
        let mut trace = super::debug::ParseTrace::new();
        let result = self.try_atom_traced(self.grammar.root, 0, 0, &mut trace);

        let final_result = match result {
            Ok(parse_result) => {
                if parse_result.end_pos == self.input.len() {
                    Ok(parse_result.value)
                } else {
                    Err(ParseError::Incomplete {
                        expected: self.input.len(),
                        actual: parse_result.end_pos,
                    })
                }
            }
            Err(e) => Err(e),
        };

        (final_result, trace)
    }

    /// Try to match an atom with tracing
    fn try_atom_traced(
        &mut self,
        atom_id: usize,
        pos: usize,
        depth: usize,
        trace: &mut super::debug::ParseTrace,
    ) -> Result<ParseResult, ParseError> {
        use super::debug::{TraceAction, TraceEntry};

        // Record entry
        trace.add(TraceEntry {
            position: pos,
            atom_id,
            action: TraceAction::Enter,
            depth,
        });

        // Check cache
        let cache_hit = self
            .cache
            .get(pos as u32, atom_id as u16)
            .map(|e| (e.success(), e.end_pos, e.ast_ref()));

        if let Some((success, end_pos, ast_ref)) = cache_hit {
            trace.add(TraceEntry {
                position: pos,
                atom_id,
                action: TraceAction::CacheHit,
                depth,
            });

            return if success {
                let cached_node = self.cached_nodes[ast_ref as usize];
                Ok(ParseResult {
                    value: cached_node,
                    end_pos: end_pos as usize,
                })
            } else {
                Err(ParseError::Failed { position: pos })
            };
        }

        // Parse the atom
        let result = self.parse_atom_uncached(atom_id, pos);

        match &result {
            Ok(r) => {
                trace.add(TraceEntry {
                    position: pos,
                    atom_id,
                    action: TraceAction::Match {
                        length: r.end_pos - pos,
                    },
                    depth,
                });
            }
            Err(_) => {
                trace.add(TraceEntry {
                    position: pos,
                    atom_id,
                    action: TraceAction::Fail,
                    depth,
                });
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::parser_dsl::{str, GrammarBuilder};
    use super::*;

    #[test]
    fn test_parse_with_rich_error_success() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "hello", &mut arena);

        let result = parser.parse_with_rich_error();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_with_rich_error_failure() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "world", &mut arena);

        let result = parser.parse_with_rich_error();
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.message.contains("Expected"));
    }

    #[test]
    fn test_parse_with_trace_success() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "hello", &mut arena);

        let (result, trace) = parser.parse_with_trace();
        assert!(result.is_ok());
        assert!(!trace.entries.is_empty());
    }

    #[test]
    fn test_parse_with_trace_failure() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "world", &mut arena);

        let (result, trace) = parser.parse_with_trace();
        assert!(result.is_err());
        // Trace should contain entries even for failed parse
        assert!(!trace.entries.is_empty());
    }

    #[test]
    fn test_trace_format() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "hello", &mut arena);

        let (_, trace) = parser.parse_with_trace();
        let formatted = trace.format(&grammar);
        assert!(formatted.contains("Enter"));
    }

    #[test]
    fn test_rich_error_format_with_source() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "world", &mut arena);

        let result = parser.parse_with_rich_error();
        if let Err(error) = result {
            let formatted = error.format_with_source("world");
            assert!(formatted.contains("line"));
            assert!(formatted.contains("column"));
        }
    }

    #[test]
    fn test_set_timeout() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "hello", &mut arena);
        parser.set_timeout_ms(1000);

        // Parsing should succeed (fast operation won't timeout)
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_max_memory() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "hello", &mut arena);
        parser.set_max_memory(1_000_000); // 1 MB

        // Parsing should succeed (uses far less than 1 MB)
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_usage() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let parser = PortableParser::new(&grammar, "hello", &mut arena);

        // Memory usage should be positive
        let usage = parser.memory_usage();
        assert!(usage > 0);
    }

    #[test]
    fn test_resource_limits_combined() {
        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "hello", &mut arena);
        parser.set_timeout_ms(1000);
        parser.set_max_memory(1_000_000);
        parser.set_max_recursion_depth(100);

        // Parsing should succeed with reasonable limits
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_with_builder() {
        #[allow(unused_imports)]
        use super::super::streaming_builder::{DebugBuilder, StreamingBuilder as _};

        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "hello", &mut arena);

        let mut builder = DebugBuilder::new();
        let result: Result<Vec<String>, _> = parser.parse_with_builder(&mut builder);

        assert!(result.is_ok());
        let events = result.unwrap();
        // Should have at least one event for the matched input
        assert!(!events.is_empty());
    }

    #[test]
    fn test_parse_with_builder_collects_strings() {
        #[allow(unused_imports)]
        use super::super::streaming_builder::{BuilderStringCollector, StreamingBuilder as _};

        let grammar = GrammarBuilder::new().rule("test", str("hello")).build();

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "hello", &mut arena);

        let mut builder = BuilderStringCollector::new();
        let result: Result<Vec<String>, _> = parser.parse_with_builder(&mut builder);

        assert!(result.is_ok());
        let strings = result.unwrap();
        // Should have collected the matched string "hello"
        assert_eq!(strings, vec!["hello"]);
    }
}
