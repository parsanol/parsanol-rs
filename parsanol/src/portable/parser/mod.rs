//! Portable PEG Parser
//!
//! This module implements the core parsing engine that can be used
//! standalone (for WASM) or integrated with Ruby FFI.
//!
//! # Architecture
//!
//! The parser uses composition to separate concerns:
//! - **ResourceGovernor**: Manages recursion depth, timeout, memory limits
//! - **DenseCache**: Packrat memoization for O(n) parsing
//! - **AstArena**: Arena allocation for AST nodes
//!
//! This separation follows the Single Responsibility Principle - each component
//! has one clear purpose.

mod config;
mod context;
mod governor;
mod simd;

#[cfg(test)]
mod tests;

pub use config::{ParserConfig, DEFAULT_MAX_INPUT_SIZE, DEFAULT_MAX_RECURSION_DEPTH};
pub use context::ParseContext;
pub use governor::ResourceGovernor;

use crate::portable::arena::AstArena;
use crate::portable::ast::{AstNode, ParseError, ParseResult};
use crate::portable::cache::{CacheEntry, DenseCache};
use crate::portable::char_class::{utf8_char_len, CharacterPattern};
use crate::portable::grammar::{Atom, Grammar};
use crate::portable::regex_cache;

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

/// The portable parser engine
///
/// # Architecture
///
/// This parser uses composition to separate concerns:
/// - **ResourceGovernor**: Manages all resource limits (recursion, timeout, memory)
/// - **DenseCache**: Packrat memoization for O(n) parsing
/// - **AstArena**: Arena allocation for AST nodes
///
/// The parser itself is just a coordinator - it doesn't manage resources directly,
/// it delegates to the appropriate component. This follows the Single Responsibility
/// Principle and makes the code more testable and maintainable.
pub struct PortableParser<'a> {
    // ========================================================================
    // Grammar and Input (immutable)
    // ========================================================================
    /// The compiled grammar
    grammar: &'a Grammar,

    /// Input string (UTF-8)
    input: &'a str,

    /// Input as bytes (for fast indexing)
    input_bytes: &'a [u8],

    // ========================================================================
    // Output (mutable)
    // ========================================================================
    /// AST arena for allocating nodes
    arena: &'a mut AstArena,

    /// Packrat cache for memoization
    cache: DenseCache,

    /// Cached AST nodes for cache hits
    cached_nodes: Vec<AstNode>,

    // ========================================================================
    // Resource Management (delegated)
    // ========================================================================
    /// Resource governor - manages all limits via composition
    governor: ResourceGovernor,
}

impl<'a> PortableParser<'a> {
    /// Create a new parser with default security limits
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
    #[inline]
    pub fn new_with_cache(
        grammar: &'a Grammar,
        input: &'a str,
        arena: &'a mut AstArena,
        cache: DenseCache,
        cached_nodes: Vec<AstNode>,
    ) -> Self {
        let governor = ResourceGovernor::new()
            .with_max_input_size(DEFAULT_MAX_INPUT_SIZE)
            .with_max_recursion_depth(DEFAULT_MAX_RECURSION_DEPTH);

        Self {
            grammar,
            input,
            input_bytes: input.as_bytes(),
            arena,
            cache,
            cached_nodes,
            governor,
        }
    }

    /// Create a new parser with custom limits
    #[inline]
    pub fn with_limits(
        grammar: &'a Grammar,
        input: &'a str,
        arena: &'a mut AstArena,
        max_input_size: usize,
        max_recursion_depth: usize,
    ) -> Self {
        let cache = DenseCache::for_input(input.len(), grammar.atom_count());
        let estimated_entries = (input.len() / 10).clamp(64, 10000);

        let governor = ResourceGovernor::new()
            .with_max_input_size(max_input_size)
            .with_max_recursion_depth(max_recursion_depth);

        Self {
            grammar,
            input,
            input_bytes: input.as_bytes(),
            arena,
            cache,
            cached_nodes: Vec::with_capacity(estimated_entries),
            governor,
        }
    }

    /// Extract the cache and cached nodes
    #[inline]
    pub fn into_cache(self) -> (DenseCache, Vec<AstNode>) {
        (self.cache, self.cached_nodes)
    }

    /// Set maximum input size
    #[inline]
    pub fn set_max_input_size(&mut self, size: usize) {
        self.governor.set_max_input_size(size);
    }

    /// Set maximum recursion depth
    #[inline]
    pub fn set_max_recursion_depth(&mut self, depth: usize) {
        self.governor.set_max_recursion_depth(depth);
    }

    /// Set timeout in milliseconds
    #[inline]
    pub fn set_timeout_ms(&mut self, timeout_ms: u64) {
        self.governor.set_timeout_ms(timeout_ms);
    }

    /// Set maximum memory
    #[inline]
    pub fn set_max_memory(&mut self, max_memory: usize) {
        self.governor.set_max_memory(max_memory);
    }

    /// Get memory usage
    #[inline]
    pub fn memory_usage(&self) -> usize {
        self.arena.memory_usage() + self.cache.memory_usage()
    }

    // ========================================================================
    // Resource Checking (delegated to governor)
    // ========================================================================

    /// Check input size against limit
    #[inline]
    fn check_input_size(&self) -> Result<(), ParseError> {
        self.governor.check_input_size(self.input.len())
    }

    /// Enter a recursive parsing context
    #[inline]
    fn enter_recursive(&mut self) -> Result<(), ParseError> {
        self.governor.enter_recursive()
    }

    /// Exit a recursive parsing context
    #[inline]
    fn exit_recursive(&mut self) {
        self.governor.exit_recursive()
    }

    /// Start the timeout timer
    #[inline]
    fn start_timeout_timer(&mut self) {
        self.governor.start_timeout_timer()
    }

    /// Check resources (timeout and memory)
    #[inline]
    fn check_resources(&mut self) -> Result<(), ParseError> {
        self.governor.check_resources(self.memory_usage())
    }

    // ========================================================================
    // Main Parse Methods
    // ========================================================================

    /// Parse the input
    #[inline]
    pub fn parse(&mut self) -> Result<AstNode, ParseError> {
        self.check_input_size()?;
        self.start_timeout_timer();

        log_debug!(
            "Starting parse: input_len={}, root_atom={}",
            self.input.len(),
            self.grammar.root
        );

        match self.try_atom(self.grammar.root, 0) {
            Ok(result) => {
                if result.end_pos == self.input.len() {
                    log_debug!("Parse successful");
                    Ok(result.value)
                } else {
                    Err(ParseError::Incomplete {
                        expected: self.input.len(),
                        actual: result.end_pos,
                    })
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Parse with end position
    #[inline]
    pub fn parse_with_end_pos(&mut self) -> Result<ParseResult, ParseError> {
        self.check_input_size()?;
        self.start_timeout_timer();
        self.try_atom(self.grammar.root, 0)
    }

    /// Parse with custom config
    pub fn parse_with_config(&mut self, config: ParserConfig) -> Result<AstNode, ParseError> {
        self.governor.set_max_input_size(config.max_input_size);
        self.governor
            .set_max_recursion_depth(config.max_recursion_depth);
        self.governor.set_timeout_ms(config.timeout_ms);
        self.governor.set_max_memory(config.max_memory);
        self.parse()
    }

    /// Parse with streaming builder
    pub fn parse_with_builder<B: super::streaming_builder::StreamingBuilder>(
        &mut self,
        builder: &mut B,
    ) -> Result<B::Output, ParseError> {
        use super::parslet_transform::to_parslet_compatible;
        use super::streaming_builder::walk_ast;

        builder
            .on_start(self.input)
            .map_err(|e| ParseError::BuilderError {
                message: e.to_string(),
            })?;

        let raw_ast = self.parse()?;
        let transformed = to_parslet_compatible(&raw_ast, self.arena, self.input);

        walk_ast(&transformed, self.arena, self.input, builder).map_err(|e| {
            ParseError::BuilderError {
                message: e.to_string(),
            }
        })?;

        builder.on_success().map_err(|e| ParseError::BuilderError {
            message: e.to_string(),
        })?;

        builder.finish().map_err(|e| ParseError::BuilderError {
            message: e.to_string(),
        })
    }

    // ========================================================================
    // Cache Management
    // ========================================================================

    #[inline(always)]
    fn store_cached_node(&mut self, node: AstNode) -> u32 {
        let idx = self.cached_nodes.len() as u32;
        self.cached_nodes.push(node);
        idx
    }

    // ========================================================================
    // Core Parsing - Try Atom
    // ========================================================================

    #[inline]
    fn try_atom(&mut self, atom_id: usize, pos: usize) -> Result<ParseResult, ParseError> {
        self.check_resources()?;

        // Check cache
        let cache_hit = self
            .cache
            .get(pos as u32, atom_id as u16)
            .map(|e| (e.success(), e.end_pos, e.ast_ref()));

        if let Some((success, end_pos, ast_ref)) = cache_hit {
            return if success {
                let cached = self.cached_nodes[ast_ref as usize];
                Ok(ParseResult {
                    value: cached,
                    end_pos: end_pos as usize,
                })
            } else {
                Err(ParseError::Failed { position: pos })
            };
        }

        // Parse uncached
        let result = self.parse_atom_uncached(atom_id, pos)?;

        // Cache result
        let ast_ref = self.store_cached_node(result.value);
        self.cache.insert(CacheEntry::new(
            pos as u32,
            atom_id as u16,
            true,
            result.end_pos as u32,
            ast_ref,
        ));

        Ok(ParseResult {
            value: self.cached_nodes[ast_ref as usize],
            end_pos: result.end_pos,
        })
    }

    #[inline]
    fn parse_atom_uncached(
        &mut self,
        atom_id: usize,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        match self.grammar.get_atom(atom_id) {
            Some(atom) => match atom {
                Atom::Str { pattern } => self.parse_str(pattern, pos),
                Atom::Re { pattern } => self.parse_re(pattern, pos),
                Atom::Sequence { atoms } => self.parse_sequence(atoms, pos),
                Atom::Alternative { atoms } => self.parse_alternative(atoms, pos),
                Atom::Repetition { atom, min, max } => {
                    self.parse_repetition(*atom, *min, *max, pos)
                }
                Atom::Named { name, atom } => self.parse_named(name, *atom, pos),
                Atom::Entity { atom } => {
                    self.enter_recursive()?;
                    let result = self.try_atom(*atom, pos);
                    self.exit_recursive();
                    result
                }
                Atom::Lookahead { atom, positive } => self.parse_lookahead(*atom, *positive, pos),
                Atom::Cut => Ok(ParseResult {
                    value: AstNode::Nil,
                    end_pos: pos,
                }),
                Atom::Ignore { atom } => {
                    let result = self.try_atom(*atom, pos)?;
                    Ok(ParseResult {
                        value: AstNode::Nil,
                        end_pos: result.end_pos,
                    })
                }
                Atom::Custom { id } => self.parse_custom(*id, pos),
            },
            None => Err(ParseError::Internal {
                message: "Invalid atom ID".to_string(),
            }),
        }
    }

    // ========================================================================
    // Atom Parsers
    // ========================================================================

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

    #[inline]
    fn parse_re(&mut self, pattern: &str, pos: usize) -> Result<ParseResult, ParseError> {
        if pos >= self.input.len() {
            return Err(ParseError::Failed { position: pos });
        }

        let b = self.input_bytes[pos];

        // Fast path for character classes
        if let Some(char_pattern) = CharacterPattern::from_pattern(pattern) {
            if char_pattern.matches(b) {
                let char_len = match char_pattern {
                    CharacterPattern::Any
                    | CharacterPattern::NonDigit
                    | CharacterPattern::NonSpace
                    | CharacterPattern::NonWord => utf8_char_len(b),
                    _ => 1,
                };
                return Ok(ParseResult {
                    value: self.arena.input_ref(pos, char_len),
                    end_pos: pos + char_len,
                });
            } else {
                return Err(ParseError::Failed { position: pos });
            }
        }

        // General case
        let regex = match regex_cache::get_or_compile(pattern) {
            Some(r) => r,
            None => {
                return Err(ParseError::Internal {
                    message: format!("Invalid regex: {}", pattern),
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

    #[inline]
    fn parse_sequence(&mut self, atoms: &[usize], pos: usize) -> Result<ParseResult, ParseError> {
        let mut current_pos = pos;
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

    #[inline]
    fn parse_repetition(
        &mut self,
        atom_id: usize,
        min: usize,
        max: Option<usize>,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        // Check for SIMD optimization
        if let Some(Atom::Re { pattern }) = self.grammar.get_atom(atom_id) {
            if let Some(char_pattern) = CharacterPattern::from_pattern(pattern) {
                return self.parse_repetition_bulk(char_pattern.predicate(), min, max, pos);
            }
        }

        let mut current_pos = pos;
        let mut count = 0;
        let mut items: Vec<AstNode> = Vec::with_capacity(min.clamp(8, 64));

        if let Some(max_count) = max {
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

    #[inline]
    fn parse_repetition_bulk(
        &mut self,
        predicate: fn(u8) -> bool,
        min: usize,
        max: Option<usize>,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        use simd::skip_while;

        let end_pos = skip_while(self.input_bytes, pos, predicate);
        let count = end_pos - pos;

        if count < min {
            return Err(ParseError::Failed { position: pos });
        }

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
        Ok(ParseResult {
            value: self.arena.input_ref(pos, actual_count),
            end_pos: actual_end,
        })
    }

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

    #[inline]
    fn parse_custom(&mut self, id: u64, pos: usize) -> Result<ParseResult, ParseError> {
        use super::custom;
        match custom::parse_custom_atom(id, self.input, pos) {
            Some(result) => {
                let value = match result.value {
                    Some(node) => node,
                    None => self.arena.input_ref(pos, result.end_pos - pos),
                };
                Ok(ParseResult {
                    value,
                    end_pos: result.end_pos,
                })
            }
            None => Err(ParseError::Failed { position: pos }),
        }
    }

    // ========================================================================
    // Rich Error Support
    // ========================================================================

    /// Parse with rich error reporting
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

    #[allow(clippy::result_large_err)]
    fn try_atom_with_error(
        &mut self,
        atom_id: usize,
        pos: usize,
        context: Option<&str>,
    ) -> Result<ParseResult, super::error::RichError> {
        use super::error::{offset_to_line_col, ErrorBuilder, RichError, Span};

        match self.try_atom(atom_id, pos) {
            Ok(result) => Ok(result),
            Err(ParseError::Failed { position }) => {
                let (line, col) = offset_to_line_col(self.input, position);
                let span = Span::at(position, line, col);
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
            Err(e) => {
                let pos = match &e {
                    ParseError::Internal { .. } => pos,
                    _ => 0,
                };
                let (line, col) = offset_to_line_col(self.input, pos);
                Err(RichError::at_position(e.to_string(), pos, line, col))
            }
        }
    }

    fn describe_atom_failure(&self, atom: Option<&Atom>, pos: usize) -> String {
        let char_at = if pos < self.input.len() {
            match self.input[pos..].chars().next() {
                Some(c) => format!("{:?}", c),
                None => "end of input".to_string(),
            }
        } else {
            "end of input".to_string()
        };

        match atom {
            Some(Atom::Str { pattern }) => format!("Expected {:?}, found {}", pattern, char_at),
            Some(Atom::Re { pattern }) => {
                format!("Expected pattern {:?}, found {}", pattern, char_at)
            }
            Some(Atom::Sequence { atoms }) => {
                format!(
                    "Failed to match sequence of {} items at {}",
                    atoms.len(),
                    char_at
                )
            }
            Some(Atom::Alternative { atoms }) => {
                format!(
                    "Expected one of {} alternatives, found {}",
                    atoms.len(),
                    char_at
                )
            }
            Some(Atom::Repetition { min, max, .. }) => {
                let max_str = max
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "∞".to_string());
                format!("Expected {}..{} repetitions at {}", min, max_str, char_at)
            }
            Some(Atom::Named { name, .. }) => format!("Failed to match {:?} at {}", name, char_at),
            Some(Atom::Lookahead { positive, .. }) => {
                if *positive {
                    format!("Positive lookahead failed at {}", char_at)
                } else {
                    format!("Negative lookahead failed at {}", char_at)
                }
            }
            _ => format!("Failed to match at {}", char_at),
        }
    }

    // ========================================================================
    // Tracing Support
    // ========================================================================

    /// Parse with tracing
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

    fn try_atom_traced(
        &mut self,
        atom_id: usize,
        pos: usize,
        depth: usize,
        trace: &mut super::debug::ParseTrace,
    ) -> Result<ParseResult, ParseError> {
        use super::debug::{TraceAction, TraceEntry};

        trace.add(TraceEntry {
            position: pos,
            atom_id,
            action: TraceAction::Enter,
            depth,
        });

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
                let cached = self.cached_nodes[ast_ref as usize];
                Ok(ParseResult {
                    value: cached,
                    end_pos: end_pos as usize,
                })
            } else {
                Err(ParseError::Failed { position: pos })
            };
        }

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
