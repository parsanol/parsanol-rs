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

#[cfg(test)]
mod tests;

pub use config::{ParserConfig, DEFAULT_MAX_INPUT_SIZE, DEFAULT_MAX_RECURSION_DEPTH};
pub use context::ParseContext;
pub use governor::ResourceGovernor;

use crate::portable::arena::AstArena;
use crate::portable::ast::{AstNode, ParseError, ParseResult};
use crate::portable::cache::{CacheEntry, DenseCache};
use crate::portable::capture_state::CaptureState;
use crate::portable::char_class::{utf8_char_len, CharacterPattern};
use crate::portable::grammar::{Atom, Grammar, RepetitionTag};
use crate::portable::regex_cache;

/// Memoized scan plans keyed by class pattern (TODO.perf/2). Grammars
/// use a handful of distinct classes; the map is capped and cleared
/// rather than growing unboundedly across parses.
fn scan_plan_for(
    pattern: &str,
    predicate: fn(u8) -> bool,
) -> std::sync::Arc<crate::portable::scan::ScanPlan> {
    use std::collections::HashMap;
    use std::sync::Arc;
    thread_local! {
        static PLANS: std::cell::RefCell<HashMap<String, Arc<crate::portable::scan::ScanPlan>>> =
            std::cell::RefCell::new(HashMap::new());
    }
    PLANS.with(|plans| {
        let mut plans = plans.borrow_mut();
        if let Some(plan) = plans.get(pattern) {
            return plan.clone();
        }
        let plan = Arc::new(crate::portable::scan::ScanPlan::from_membership(predicate));
        if plans.len() >= 512 {
            plans.clear();
        }
        plans.insert(pattern.to_string(), plan.clone());
        plan
    })
}

/// Atoms whose subtree contains a Dynamic atom. Dynamic resolution
/// depends on capture state (GH-76), which memoization does not key
/// on — memoizing these atoms can replay a stale outcome (a different
/// capture context at the same position can resolve differently).
/// Empty when the grammar has no Dynamic atoms (TODO.perf/5).
fn dynamic_dependence(grammar: &Grammar) -> Vec<bool> {
    let has_dynamic = grammar
        .atoms
        .iter()
        .any(|a| matches!(a, Atom::Dynamic { .. }));
    if !has_dynamic {
        return Vec::new();
    }

    let n = grammar.atoms.len();
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, atom) in grammar.atoms.iter().enumerate() {
        let mut children = Vec::new();
        match atom {
            Atom::Sequence { atoms } | Atom::Alternative { atoms } => children.extend(atoms),
            Atom::Repetition { atom, .. }
            | Atom::Named { atom, .. }
            | Atom::Entity { atom }
            | Atom::Lookahead { atom, .. }
            | Atom::Ignore { atom }
            | Atom::Capture { atom, .. }
            | Atom::Scope { atom } => children.push(*atom),
            _ => {}
        }
        for c in children {
            if c < n {
                parents[c].push(i);
            }
        }
    }

    let mut dep = vec![false; n];
    let mut stack: Vec<usize> = grammar
        .atoms
        .iter()
        .enumerate()
        .filter_map(|(i, a)| matches!(a, Atom::Dynamic { .. }).then_some(i))
        .collect();
    while let Some(a) = stack.pop() {
        if dep[a] {
            continue;
        }
        dep[a] = true;
        stack.extend(parents[a].iter().copied());
    }
    dep
}

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

    // Deepest-failure tracking for cause diagnostics: the furthest
    // position any terminal failed at, and the labels expected there.
    deepest_failure_pos: usize,
    has_failure: bool,
    expected_labels: Vec<String>,

    // ========================================================================
    // Output (mutable)
    // ========================================================================
    /// AST arena for allocating nodes
    arena: &'a mut AstArena,

    /// Packrat cache for memoization (inlined node data)
    cache: DenseCache,

    // ========================================================================
    // Resource Management (delegated)
    // ========================================================================
    /// Resource governor - manages all limits via composition
    governor: ResourceGovernor,

    // ========================================================================
    // Capture State
    // ========================================================================
    /// Capture state for named captures
    capture_state: CaptureState,

    /// Enable arena rollback on failed alternatives.
    /// This is set when the cache is effectively empty (no memoization),
    /// allowing safe cleanup of garbage from failed parse branches.
    rollback_on_failure: bool,

    /// Per-atom dynamic dependence (empty when the grammar has no
    /// Dynamic atoms): memoization is skipped for these atoms.
    dynamic_dependent: Vec<bool>,

    /// Stable arena backing snapshot-marked cache entries (incremental
    /// reparsing, TODO.perf/4). Hits on snapshot entries adopt their
    /// node data into the live arena.
    snapshot_arena: Option<&'a AstArena>,

    /// Per-atom dispatch counts (parsanol-rs#100 item 2): None until
    /// `enable_profiling`, then one counter per atom.
    dispatch_counts: Option<Box<[u64]>>,
}

impl<'a> PortableParser<'a> {
    /// Create a new parser with default security limits
    #[inline]
    pub fn new(grammar: &'a Grammar, input: &'a str, arena: &'a mut AstArena) -> Self {
        if !super::dynamic::in_dynamic_dispatch() {
            super::dynamic::begin_parse(input);
        }
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
    ) -> Self {
        Self::new_with_cache_and_snap(grammar, input, arena, cache, None)
    }

    /// Create a parser with a pre-existing cache whose snapshot-marked
    /// entries are backed by `snapshot_arena` (incremental reparsing,
    /// TODO.perf/4): hits on those entries adopt their node data into
    /// the live arena.
    #[inline]
    pub fn new_with_cache_and_snap(
        grammar: &'a Grammar,
        input: &'a str,
        arena: &'a mut AstArena,
        cache: DenseCache,
        snapshot_arena: Option<&'a AstArena>,
    ) -> Self {
        let governor = ResourceGovernor::new()
            .with_max_input_size(DEFAULT_MAX_INPUT_SIZE)
            .with_max_recursion_depth(DEFAULT_MAX_RECURSION_DEPTH);

        // Enable rollback when cache is effectively empty (parse_fresh scenario).
        // This allows arena cleanup on failed alternatives without corrupting cache.
        let rollback_on_failure = cache.is_empty();

        Self {
            grammar,
            input,
            input_bytes: input.as_bytes(),
            deepest_failure_pos: 0,
            has_failure: false,
            expected_labels: Vec::new(),
            arena,
            cache,
            governor,
            capture_state: CaptureState::new(),
            rollback_on_failure,
            dynamic_dependent: dynamic_dependence(grammar),
            snapshot_arena,
            dispatch_counts: None,
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

        let governor = ResourceGovernor::new()
            .with_max_input_size(max_input_size)
            .with_max_recursion_depth(max_recursion_depth);

        Self {
            grammar,
            input,
            input_bytes: input.as_bytes(),
            deepest_failure_pos: 0,
            has_failure: false,
            expected_labels: Vec::new(),
            arena,
            cache,
            governor,
            capture_state: CaptureState::new(),
            rollback_on_failure: false,
            dynamic_dependent: dynamic_dependence(grammar),
            snapshot_arena: None,
            dispatch_counts: None,
        }
    }

    /// Extract the cache
    #[inline]
    pub fn into_cache(self) -> DenseCache {
        self.cache
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

    /// Get a reference to the capture state
    #[inline]
    pub fn capture_state(&self) -> &CaptureState {
        &self.capture_state
    }

    /// Get a mutable reference to the capture state
    #[inline]
    pub fn capture_state_mut(&mut self) -> &mut CaptureState {
        &mut self.capture_state
    }

    /// Start counting dispatches per atom (parsanol-rs#100): one
    /// counter per grammar atom, incremented every time the atom is
    /// attempted at any position. Off by default; enabling allocates
    /// one zeroed u64 per atom and adds a single counted branch to
    /// the dispatch path.
    pub fn enable_profiling(&mut self) {
        let n = self.grammar.atom_count();
        self.dispatch_counts = Some(vec![0u64; n].into_boxed_slice());
    }

    /// Per-atom dispatch counts, indexed by atom id, when profiling
    /// is enabled.
    pub fn dispatch_counts(&self) -> Option<&[u64]> {
        self.dispatch_counts.as_deref()
    }

    /// Atom ids sorted by dispatch count, hottest first ( profiling
    /// must be enabled; empty otherwise).
    pub fn profile_summary(&self) -> Vec<(usize, u64)> {
        match &self.dispatch_counts {
            None => Vec::new(),
            Some(counts) => {
                let mut summary: Vec<(usize, u64)> = counts.iter().copied().enumerate().collect();
                summary.sort_by_key(|&(_, c)| std::cmp::Reverse(c));
                summary
            }
        }
    }

    /// Parse from a specific position (for dynamic atom support)
    pub fn parse_from_pos(&mut self, pos: usize) -> Result<ParseResult, ParseError> {
        let result = self.try_atom(self.grammar.root, pos)?;
        Ok(ParseResult {
            value: result.value,
            end_pos: result.end_pos,
            capture_state: Some(self.capture_state.clone()),
        })
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
        self.governor
            .check_resources_lazy(|| self.arena.memory_usage() + self.cache.memory_usage())
    }

    // ========================================================================
    // Main Parse Methods
    // ========================================================================

    /// Deepest terminal failure collected during the last parse.
    #[inline]
    pub fn failure_diagnostics(&self) -> Option<(usize, Vec<String>)> {
        self.has_failure
            .then(|| (self.deepest_failure_pos, self.expected_labels.clone()))
    }

    /// Parse the input
    #[inline]
    /// Parse, returning the RAW tagged tree (parsanol-rs#100 item 4).
    ///
    /// # The raw tree shape (supported, stable API)
    ///
    /// The returned `AstNode` graph is the engine's own value model,
    /// readable against the arena this parser was built with:
    ///
    /// - `InputRef { offset, length }` — a matched input span; the
    ///   text is `&input[offset..offset + length]`. Zero-copy.
    /// - `Array { pool_index, length }` — a tagged envelope whose
    ///   first element is the tag `":sequence"` or `":repetition"`
    ///   (an interned `StringRef`), followed by the children, one per
    ///   matched child atom (sequences) or iteration (repetitions).
    ///   Fetch items with `arena.get_array(pool_index, length)`.
    /// - `Hash { pool_index, length }` — a named capture: one pair,
    ///   the key is the capture name. `arena.get_hash_items`.
    /// - `StringRef { pool_index }` — engine-generated literals
    ///   (e.g. an empty sequence flattening to `""`).
    /// - `Nil` — an ignored atom's contribution.
    ///
    /// Consumers that walk this shape directly skip the parslet
    /// normalization (`to_parslet_compatible`), which measures
    /// 12–14% of some pipelines. The parslet-compatible view remains
    /// available wherever the engine applies it (the Ruby bridge,
    /// `parse_with_builder`).
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
            Err(mut e) => {
                if self.has_failure {
                    // The cause points at the deepest failure, not the
                    // outermost construct that propagated it — the
                    // position a reader actually wants to look at.
                    if let ParseError::Failed { position } = &mut e {
                        *position = self.deepest_failure_pos;
                    }
                }
                Err(e)
            }
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
    // Core Parsing - Try Atom
    // ========================================================================

    /// Try to parse an atom at a given position.
    ///
    /// This is the core parsing method that attempts to match an atom
    /// at the specified position in the input. It uses packrat memoization
    /// to cache results and avoid redundant parsing.
    ///
    /// # Packrat Memoization
    ///
    /// Both successful AND failed parses are cached. Caching failures is
    /// crucial for PEG parsing performance, especially with grammars that
    /// have many alternatives (like EXPRESS with 2273 atoms).
    #[inline]
    pub fn try_atom(&mut self, atom_id: usize, pos: usize) -> Result<ParseResult, ParseError> {
        if let Some(counts) = &mut self.dispatch_counts {
            counts[atom_id] += 1;
        }
        self.check_resources()?;

        // Skip cache for atoms that don't benefit from memoization
        // (terminals, pass-through wrappers). This saves significant memory
        // since most atoms in a grammar are Re/Str terminals.
        //
        // Dynamic-dependent atoms are context-dependent: their outcome
        // varies with capture state, which the memo key ignores (GH-76).
        if self.grammar.is_no_cache(atom_id)
            || self.dynamic_dependent.get(atom_id).is_some_and(|&dep| dep)
        {
            return self.parse_atom_uncached(atom_id, pos);
        }

        // Check cache
        let cache_hit = self
            .cache
            .get(pos as u32, atom_id as u16, self.arena.generation())
            .map(|e| (e.success, e.end_pos, e.to_node(), e.is_snapshot()));

        if let Some((success, end_pos, cached_node, is_snapshot)) = cache_hit {
            return if success {
                let value = if is_snapshot {
                    match (self.snapshot_arena, &cached_node) {
                        (
                            Some(snap),
                            AstNode::Array { .. }
                            | AstNode::Hash { .. }
                            | AstNode::StringRef { .. },
                        ) => self.arena.adopt_node(snap, &cached_node),
                        _ => cached_node,
                    }
                } else {
                    cached_node
                };
                Ok(ParseResult {
                    value,
                    end_pos: end_pos as usize,
                    capture_state: None,
                })
            } else {
                // Cached failure - this is important for PEG performance!
                // Without caching failures, we'd re-parse failed alternatives every time
                Err(ParseError::Failed { position: pos })
            };
        }

        // Parse uncached
        match self.parse_atom_uncached(atom_id, pos) {
            Ok(result) => {
                // Cache successful result with inlined node data
                self.cache.insert(CacheEntry::from_node(
                    pos as u32,
                    atom_id as u16,
                    result.end_pos as u32,
                    &result.value,
                    self.arena.generation(),
                ));

                Ok(result)
            }
            Err(e) => {
                // CRITICAL: Cache failures too!
                // Without this, failed alternatives are re-parsed exponentially
                // This is the key to packrat parser performance
                self.cache
                    .insert(CacheEntry::failure(pos as u32, atom_id as u16));
                Err(e)
            }
        }
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
                Atom::Repetition {
                    atom,
                    min,
                    max,
                    tag,
                } => self.parse_repetition(*atom, *min, *max, *tag, pos),
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
                    capture_state: None,
                }),
                Atom::Ignore { atom } => {
                    let result = self.try_atom(*atom, pos)?;
                    Ok(ParseResult {
                        value: AstNode::Nil,
                        end_pos: result.end_pos,
                        capture_state: None,
                    })
                }
                Atom::Custom { id } => self.parse_custom(*id, pos),
                Atom::Capture { name, atom } => self.parse_capture(name, *atom, pos),
                Atom::Scope { atom } => self.parse_scope(*atom, pos),
                Atom::Dynamic { callback_id } => self.parse_dynamic(*callback_id, pos),
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
    /// Record a terminal failure for cause diagnostics. Only the
    /// deepest position's expectations are kept, mirroring how the
    /// Ruby engine's reporter collects the expected set.
    fn note_failure(&mut self, pos: usize, label: String) {
        if !self.has_failure || pos > self.deepest_failure_pos {
            self.has_failure = true;
            self.deepest_failure_pos = pos;
            self.expected_labels.clear();
            self.expected_labels.push(label);
        } else if pos == self.deepest_failure_pos && !self.expected_labels.contains(&label) {
            self.expected_labels.push(label);
        }
    }

    #[inline]
    fn parse_str(&mut self, pattern: &str, pos: usize) -> Result<ParseResult, ParseError> {
        let pattern_bytes = pattern.as_bytes();
        let pattern_len = pattern_bytes.len();
        let end = pos + pattern_len;

        let str_label = format!("'{}'", pattern);
        if end > self.input.len() {
            self.note_failure(pos, str_label);
            return Err(ParseError::Failed { position: pos });
        }

        let slice = &self.input_bytes[pos..end];
        if slice == pattern_bytes {
            Ok(ParseResult {
                value: self.arena.input_ref(pos, pattern_len),
                end_pos: end,
                capture_state: None,
            })
        } else {
            self.note_failure(pos, str_label);
            Err(ParseError::Failed { position: pos })
        }
    }

    #[inline]
    fn parse_re(&mut self, pattern: &str, pos: usize) -> Result<ParseResult, ParseError> {
        let re_label = pattern.to_string();
        if pos >= self.input.len() {
            self.note_failure(pos, re_label);
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
                    capture_state: None,
                });
            } else {
                self.note_failure(pos, re_label);
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
                    capture_state: None,
                });
            }
        }

        self.note_failure(pos, re_label);
        Err(ParseError::Failed { position: pos })
    }

    #[inline]
    fn parse_sequence(&mut self, atoms: &[usize], pos: usize) -> Result<ParseResult, ParseError> {
        let mut current_pos = pos;
        let mut items = Vec::with_capacity(atoms.len());

        // A failed element discards the captures of every earlier
        // element: the sequence never matched (GH-76 follow-up —
        // capture writes must not leak across failed branches).
        self.capture_state.push_scope();
        for &atom_id in atoms {
            let result = match self.try_atom(atom_id, current_pos) {
                Ok(r) => r,
                Err(e) => {
                    self.capture_state.pop_scope();
                    return Err(e);
                }
            };
            items.push(result.value);
            current_pos = result.end_pos;
        }
        self.capture_state.commit_scope();

        // Tag the array with :sequence for proper transformation
        let (pool_idx, len) = self.arena.store_tagged_array(":sequence", &items);
        Ok(ParseResult {
            value: AstNode::Array {
                pool_index: pool_idx,
                length: len,
            },
            end_pos: current_pos,
            capture_state: None,
        })
    }

    #[inline]
    fn parse_alternative(
        &mut self,
        atoms: &[usize],
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        // When rollback_on_failure is true (parse_fresh scenario), we checkpoint
        // before each atom and rollback on failure. This keeps arena clean since
        // there's no cache to corrupt.
        //
        // When rollback_on_failure is false (normal parse with cache), we don't
        // rollback because failed branches may have valid cache entries that
        // reference arena data - rollback would invalidate those references.
        if self.rollback_on_failure {
            for &atom_id in atoms {
                let cp = self.arena.checkpoint();
                self.capture_state.push_scope();
                if let Ok(result) = self.try_atom(atom_id, pos) {
                    self.capture_state.commit_scope();
                    return Ok(result);
                }
                self.capture_state.pop_scope();
                self.arena.rollback(cp);
            }
            return Err(ParseError::Failed { position: pos });
        }

        // Normal path: no arena rollback (cache protects against
        // corruption), but captures made in a failed branch are still
        // discarded — they belong to a branch that never matched.
        for &atom_id in atoms {
            self.capture_state.push_scope();
            match self.try_atom(atom_id, pos) {
                Ok(result) => {
                    self.capture_state.commit_scope();
                    return Ok(result);
                }
                Err(_) => {
                    self.capture_state.pop_scope();
                }
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
        tag: RepetitionTag,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        // Check for SIMD optimization
        if let Some(Atom::Re { pattern }) = self.grammar.get_atom(atom_id) {
            if let Some(char_pattern) = CharacterPattern::from_pattern(pattern) {
                let bulk_label = pattern.to_string();
                return self
                    .parse_repetition_bulk(pattern, char_pattern.predicate(), min, max, tag, pos)
                    .map_err(|e| {
                        if matches!(e, ParseError::Failed { .. }) {
                            self.note_failure(pos, bulk_label.clone());
                        }
                        e
                    });
            }
        }

        let mut current_pos = pos;
        let mut count = 0;
        let mut items: Vec<AstNode> = Vec::with_capacity(min.clamp(8, 64));

        // The whole repetition owns its captures: below-min failure
        // discards them, and each optional iteration that fails to
        // match discards only its own (the successful prefix keeps
        // its captures — a partial run is a successful repetition).
        self.capture_state.push_scope();
        if let Some(max_count) = max {
            while count < max_count {
                self.capture_state.push_scope();
                match self.try_atom(atom_id, current_pos) {
                    Ok(result) => {
                        self.capture_state.commit_scope();
                        items.push(result.value);
                        current_pos = result.end_pos;
                        count += 1;
                    }
                    Err(_) => {
                        self.capture_state.pop_scope();
                        break;
                    }
                }
            }
        } else {
            loop {
                self.capture_state.push_scope();
                match self.try_atom(atom_id, current_pos) {
                    Ok(result) => {
                        self.capture_state.commit_scope();
                        items.push(result.value);
                        current_pos = result.end_pos;
                        count += 1;
                    }
                    Err(_) => {
                        self.capture_state.pop_scope();
                        break;
                    }
                }
            }
        }

        if count < min {
            self.capture_state.pop_scope();
            return Err(ParseError::Failed { position: pos });
        }
        self.capture_state.commit_scope();

        // A PRESENT optional flattens to its value in every context
        // ([:maybe, v] -> v), so only the absent case needs the tag
        if tag == RepetitionTag::Maybe && items.len() == 1 {
            let value = items.pop().unwrap_or(AstNode::Nil);
            return Ok(ParseResult {
                value,
                end_pos: current_pos,
                capture_state: None,
            });
        }

        // Tag the array for proper transformation: :maybe flattens to
        // nil-or-value, :repetition flattens to an array
        let tag_str = match tag {
            RepetitionTag::Maybe => ":maybe",
            RepetitionTag::Repetition => ":repetition",
        };
        let (pool_idx, len) = self.arena.store_tagged_array(tag_str, &items);
        Ok(ParseResult {
            value: AstNode::Array {
                pool_index: pool_idx,
                length: len,
            },
            end_pos: current_pos,
            capture_state: None,
        })
    }

    #[inline]
    fn parse_repetition_bulk(
        &mut self,
        pattern: &str,
        predicate: fn(u8) -> bool,
        min: usize,
        max: Option<usize>,
        tag: RepetitionTag,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        // One plan per distinct class pattern (TODO.perf/2): the
        // comparison-shaped scan replaces the per-byte skip loop.
        let plan = scan_plan_for(pattern, predicate);

        let end_pos = plan.scan_run_bytewise(self.input_bytes, pos);
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
        let value = if tag == RepetitionTag::Maybe {
            if actual_count == 1 {
                // Present optional flattens to its value: no tag needed
                self.arena.input_ref(pos, actual_count)
            } else {
                // Absent optional keeps the tag so downstream flattening
                // yields nil (named) or "" (unnamed), never an empty match
                let (pool_idx, len) = self.arena.store_tagged_array(":maybe", &[]);
                AstNode::Array {
                    pool_index: pool_idx,
                    length: len,
                }
            }
        } else {
            self.arena.input_ref(pos, actual_count)
        };
        Ok(ParseResult {
            value,
            end_pos: actual_end,
            capture_state: None,
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
            capture_state: None,
        })
    }

    #[inline]
    fn parse_lookahead(
        &mut self,
        atom_id: usize,
        positive: bool,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        // A lookahead inspects without consuming: captures made inside
        // its body never persist, positive or negative.
        self.capture_state.push_scope();
        let matches = self.try_atom(atom_id, pos).is_ok();
        self.capture_state.pop_scope();
        if matches == positive {
            Ok(ParseResult {
                value: AstNode::Nil,
                end_pos: pos,
                capture_state: None,
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
                    capture_state: None,
                })
            }
            None => Err(ParseError::Failed { position: pos }),
        }
    }

    /// Parse a capture atom
    ///
    /// Captures the result of parsing the inner atom under the given name.
    #[inline]
    fn parse_capture(
        &mut self,
        name: &str,
        atom_id: usize,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        let result = self.try_atom(atom_id, pos)?;

        // Store the capture: the span keeps text access cheap; the
        // parsed subtree travels with it because capture semantics
        // expose the TREE to dynamic blocks (Capture#apply parity,
        // coradoc block_style_exact verbatim cast). The subtree is
        // materialized to a self-contained portable value here so it
        // survives fragment-parser crossings without arena ties.
        let capture_value = super::capture_state::CaptureValue::new(pos, result.end_pos - pos);
        let shaped =
            super::parslet_transform::to_parslet_compatible(&result.value, self.arena, self.input);
        let node_value = super::transform::ast_to_value(&shaped, self.arena, self.input);
        let fingerprint = super::capture_state::value_fingerprint(&node_value);
        self.capture_state
            .store_with_node(name, capture_value, node_value, fingerprint);

        // Return result with capture state
        Ok(ParseResult {
            value: result.value,
            end_pos: result.end_pos,
            capture_state: Some(self.capture_state.clone()),
        })
    }

    /// Parse a scope atom
    ///
    /// Creates an isolated scope for captures. Any captures made inside
    /// will be discarded when the scope exits (unless explicitly promoted).
    #[inline]
    fn parse_scope(&mut self, atom_id: usize, pos: usize) -> Result<ParseResult, ParseError> {
        // Push a new scope
        self.capture_state.push_scope();

        // Parse the inner atom
        let result = self.try_atom(atom_id, pos);

        // Pop the scope (discards inner captures)
        self.capture_state.pop_scope();

        // Return result with remaining capture state
        result.map(|r| ParseResult {
            value: r.value,
            end_pos: r.end_pos,
            capture_state: Some(self.capture_state.clone()),
        })
    }

    /// Parse a dynamic atom
    ///
    /// Invokes a registered callback to determine which atom to parse.
    #[inline]
    fn parse_dynamic(&mut self, callback_id: u64, pos: usize) -> Result<ParseResult, ParseError> {
        use super::dynamic::{with_dynamic_callback, DynamicContext};

        // Fragment recursion guard: a grammar whose dispatch never
        // converges (e.g. captures that stay invisible to the block)
        // recurses through nested fragment parses forever. Hard-fail
        // loudly instead of hanging (GH-76 follow-up). Depth and call
        // budget live in `dynamic` so both engines share one policy;
        // the RAII guard decrements on every exit path.
        let _guard = super::dynamic::enter_dynamic().ok_or(ParseError::Failed { position: pos })?;

        // Dispatch cache (parsanol-ruby#80): a deterministic block's
        // fragment is a pure function of (input, pos, captures) — the
        // exact key below. A hit skips the host round-trip (block
        // call, atom JSON, grammar rebuild) and replays the recorded
        // capture writes — no context materialization at all.
        if let Some((fragment, root, writes)) =
            super::dynamic::cached_fragment(callback_id, pos, &self.capture_state, self.input)
        {
            for (name, text) in writes {
                self.capture_state
                    .store(&name, super::capture_state::CaptureValue::text(text));
            }
            return self.parse_fragment(&fragment, root, pos);
        }

        // Create context for callback. Capture subtrees (materialized
        // at capture time, self-contained) ride along so the block
        // reads the parsed TREE (Capture#apply parity).
        let node_values = self.capture_state.node_values();
        let ctx = DynamicContext::with_node_values(
            self.input,
            pos,
            self.capture_state.clone(),
            node_values,
        );

        // Invoke callback: a fragment grammar (self-consistent atom
        // indices, the shape host bridges produce) wins over a single
        // index-free atom, which is appended to a grammar clone.
        let grammar = self.grammar;
        let mut resolved_fragment = false;
        let (temp_grammar, temp_atom_id) = with_dynamic_callback(callback_id, |cb| {
            if let Some((fragment, root)) = cb.resolve_fragment(&ctx) {
                resolved_fragment = true;
                return Some((fragment, root));
            }
            cb.resolve(&ctx).map(|atom| {
                let mut g = grammar.clone();
                let id = g.add_atom(atom);
                (g, id)
            })
        })
        .ok_or(ParseError::Failed { position: pos })?;

        // Writes the block made to its context (parsanol-ruby#80)
        // land in the enclosing capture scope: a failed enclosing
        // branch discards them with everything else the branch
        // captured, and later blocks read them through the seeded
        // state below. They are recorded with the cached fragment so
        // hits replay them.
        let writes = super::dynamic::take_pending_writes();
        for (name, text) in &writes {
            self.capture_state
                .store(name, super::capture_state::CaptureValue::text(text.clone()));
        }

        // Cache ONLY self-consistent fragments (small host-atom
        // subtrees). The resolve path builds `grammar.clone() + atom`
        // — a full copy of the registered grammar PER DISPATCH
        // POSITION; caching those retained megabytes per distinct
        // document (parsanol-ruby#84).
        if resolved_fragment {
            let stored = super::dynamic::store_dispatch_fragment(
                callback_id,
                pos,
                &self.capture_state,
                self.input,
                temp_grammar,
                temp_atom_id,
                writes,
            );
            self.parse_fragment(&stored, temp_atom_id, pos)
        } else {
            self.parse_fragment(&temp_grammar, temp_atom_id, pos)
        }
    }

    /// Parse a resolved fragment at `pos` against a temporary parser
    /// seeded with the parent's captures, merging results and adopting
    /// the subtree into the parent arena (GH-76). Shared by the
    /// resolved path and dispatch-cache hits.
    fn parse_fragment(
        &mut self,
        temp_grammar: &super::grammar::Grammar,
        temp_atom_id: usize,
        pos: usize,
    ) -> Result<ParseResult, ParseError> {
        let mut temp_arena = AstArena::for_input(self.input.len());
        let mut temp_parser = PortableParser::new(temp_grammar, self.input, &mut temp_arena);
        for name in self.capture_state.names() {
            if let Some(value) = self.capture_state.get(name) {
                temp_parser.capture_state.store(name, value);
            }
            // Capture subtrees cross the fragment boundary too: nested
            // dynamic dispatches inside the fragment read the same
            // parsed trees the parent's blocks would (capture parity).
            if let Some(nc) = self.capture_state.get_node(name) {
                temp_parser.capture_state.store_with_node(
                    name,
                    self.capture_state.get(name).expect("text companion"),
                    nc.value,
                    nc.fingerprint,
                );
            }
        }

        let result = temp_parser.try_atom(temp_atom_id, pos)?;

        // Merge captures from temp parser
        for name in temp_parser.capture_state.names() {
            if let Some(value) = temp_parser.capture_state.get(name) {
                self.capture_state.store(name, value);
            }
            if let Some(nc) = temp_parser.capture_state.get_node(name) {
                self.capture_state.store_with_node(
                    name,
                    temp_parser.capture_state.get(name).expect("text companion"),
                    nc.value,
                    nc.fingerprint,
                );
            }
        }

        // The subtree was built in the fragment's arena; pool-backed
        // nodes (arrays, hashes, interned strings) must be adopted
        // into the parent arena or they dangle (GH-76).
        let value = self.arena.adopt_node(&temp_arena, &result.value);

        Ok(ParseResult {
            value,
            end_pos: result.end_pos,
            capture_state: Some(self.capture_state.clone()),
        })
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

        // Skip cache for atoms that don't benefit from memoization
        if self.grammar.is_no_cache(atom_id) {
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
            return result;
        }

        let cache_hit = self
            .cache
            .get(pos as u32, atom_id as u16, self.arena.generation())
            .map(|e| (e.success, e.end_pos, e.to_node()));

        if let Some((success, end_pos, cached_node)) = cache_hit {
            trace.add(TraceEntry {
                position: pos,
                atom_id,
                action: TraceAction::CacheHit,
                depth,
            });

            return if success {
                Ok(ParseResult {
                    value: cached_node,
                    end_pos: end_pos as usize,
                    capture_state: None,
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
