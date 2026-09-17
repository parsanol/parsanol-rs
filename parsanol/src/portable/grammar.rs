//! Grammar types for Parsanol
//!
//! This module defines the in-memory representation of parsed grammars.
//! Grammars are serialized to JSON from Ruby and deserialized here.

use crate::portable::grammar_analysis::{GrammarAnalyzer, GrammarWarning};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result tag for repetition atoms.
///
/// Parslet distinguishes `.maybe` (tag `:maybe`: nil-or-value) from
/// `.repeat(0, 1)` (tag `:repetition`: empty-or-single-element array) when
/// flattening results. The tag controls which one the parser emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepetitionTag {
    /// Plain repetition: flattens to an array of matched values
    #[default]
    Repetition,
    /// Optional match (`.maybe`): flattens to nil-or-value
    Maybe,
}

/// Atom types that make up a grammar
///
/// These correspond to the different parsanol atom types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Atom {
    /// Match a literal string
    Str {
        /// The string pattern to match
        pattern: String,
    },

    /// Match a regular expression pattern
    Re {
        /// The regex pattern to match
        pattern: String,
    },

    /// Match multiple atoms in sequence
    Sequence {
        /// Indices into atoms array
        atoms: Vec<usize>,
    },

    /// Try alternatives in order
    Alternative {
        /// Indices into atoms array
        atoms: Vec<usize>,
    },

    /// Repeat an atom (greedy, with min/max)
    Repetition {
        /// Index into atoms array
        atom: usize,
        /// Minimum number of repetitions
        min: usize,
        /// Maximum number of repetitions (None = unlimited)
        max: Option<usize>,
        /// Result tag: `.maybe` flattens to nil-or-value, plain repetition
        /// flattens to an array
        #[serde(default)]
        tag: RepetitionTag,
    },

    /// Name the result
    Named {
        /// The name to give the result
        name: String,
        /// Index into atoms array
        atom: usize,
    },

    /// Reference to another atom (lazy evaluation)
    Entity {
        /// Index into atoms array
        atom: usize,
    },

    /// Lookahead (doesn't consume input)
    Lookahead {
        /// Index into atoms array
        atom: usize,
        /// Whether this is a positive lookahead
        positive: bool,
    },

    /// Atomic predicate (cut)
    ///
    /// Once this matches, backtracking past this point is prevented.
    Cut,

    /// Ignore the result
    ///
    /// Matches the inner atom but discards the result (returns Nil).
    /// Useful for whitespace, delimiters, etc.
    Ignore {
        /// Index into atoms array
        atom: usize,
    },

    /// Capture matched text with a name
    ///
    /// Stores the matched text in the capture state with the given name.
    /// The capture can be referenced later by dynamic atoms or retrieved
    /// after parsing.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Capture "foo" as :keyword
    /// Atom::Capture {
    ///     name: "keyword".to_string(),
    ///     atom: str_atom_index,
    /// }
    /// ```
    Capture {
        /// The name for this capture
        name: String,
        /// Index into atoms array
        atom: usize,
    },

    /// Create an isolated capture scope
    ///
    /// Captures made within this scope are discarded when the scope ends.
    /// Useful for lookahead-like patterns where inner captures shouldn't
    /// pollute the outer state.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Inner captures are isolated
    /// Atom::Scope {
    ///     atom: inner_atom_index,
    /// }
    /// ```
    Scope {
        /// Index into atoms array
        atom: usize,
    },

    /// Dynamic atom resolution via callback
    ///
    /// At parse time, invokes the registered callback to determine which
    /// atom to parse. The callback receives the current parsing context
    /// (input, position, captures) and returns an atom to parse.
    ///
    /// # Use Cases
    ///
    /// - Context-sensitive keywords
    /// - Parser switching based on captures
    /// - Conditional parsing logic
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Register a callback that returns different atoms based on captures
    /// let callback_id = register_dynamic_callback(Box::new(MyResolver));
    ///
    /// Atom::Dynamic {
    ///     callback_id,
    /// }
    /// ```
    Dynamic {
        /// Unique identifier for the registered callback
        callback_id: u64,
    },

    /// Custom atom extension point
    ///
    /// References a custom parsing implementation registered via
    /// `parsanol::portable::custom::register_custom_atom()`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use parsanol::portable::custom::{CustomAtom, CustomResult, register_custom_atom};
    ///
    /// struct MyMatcher;
    /// impl CustomAtom for MyMatcher {
    ///     fn parse(&self, input: &str, pos: usize) -> Option<CustomResult> {
    ///         // Custom parsing logic
    ///         None
    ///     }
    ///     fn description(&self) -> &str { "my matcher" }
    /// }
    ///
    /// let id = register_custom_atom(1000, Box::new(MyMatcher));
    /// let atom = Atom::Custom { id };
    /// ```
    Custom {
        /// Unique identifier for the custom atom
        id: u64,
    },
}

/// A complete grammar
///
/// Contains all atoms and the root atom index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grammar {
    /// All atoms in the grammar (referenced by index)
    pub atoms: Vec<Atom>,

    /// Index of the root atom
    pub root: usize,

    /// Atoms excluded from packrat caching. Empty: every atom kind is a pure
    /// function of (input, position), so all results are memoizable. Memory is
    /// bounded by the `DenseCache` entry cap, not by exempting atom kinds.
    /// Computed during `optimize()` and skipped by serde.
    #[serde(skip, default)]
    pub no_cache: Vec<bool>,
}

impl Grammar {
    /// Create a new empty grammar
    #[inline]
    pub fn new() -> Self {
        Self {
            atoms: Vec::new(),
            root: 0,
            no_cache: Vec::new(),
        }
    }

    /// Add an atom and return its index
    #[inline]
    pub fn add_atom(&mut self, atom: Atom) -> usize {
        let idx = self.atoms.len();
        self.atoms.push(atom);
        idx
    }

    /// Get an atom by index
    #[inline]
    pub fn get_atom(&self, idx: usize) -> Option<&Atom> {
        self.atoms.get(idx)
    }

    /// Get a mutable atom by index
    #[inline]
    pub fn get_atom_mut(&mut self, idx: usize) -> Option<&mut Atom> {
        self.atoms.get_mut(idx)
    }

    /// Get the root atom
    #[inline]
    pub fn root_atom(&self) -> Option<&Atom> {
        self.atoms.get(self.root)
    }

    /// Get total atom count
    #[inline]
    pub fn atom_count(&self) -> usize {
        self.atoms.len()
    }

    /// Get count of atoms that need packrat caching (not marked no_cache)
    pub fn cacheable_atom_count(&self) -> usize {
        self.no_cache.iter().filter(|&&nc| !nc).count()
    }

    /// Get count of atoms that skip packrat caching
    pub fn no_cache_atom_count(&self) -> usize {
        self.no_cache.iter().filter(|&&nc| nc).count()
    }

    /// Check if an atom doesn't need packrat caching
    ///
    /// Returns true if the atom is a terminal or pass-through that's O(1) to
    /// evaluate, so caching would waste memory without saving work.
    #[inline]
    pub fn is_no_cache(&self, atom_id: usize) -> bool {
        self.no_cache.get(atom_id).copied().unwrap_or(false)
    }

    /// Compute which atoms don't need packrat caching.
    ///
    /// Every atom kind is a pure function of (input, position): successes and
    /// failures alike are deterministic, so every result is memoizable.
    /// Restricting memoization to a few atom kinds makes large inputs
    /// super-linear (parsanol-ruby#52: ~99% of CPU samples in
    /// try_atom/parse_atom_uncached with almost no cache traffic), because
    /// non-memoized subtrees are re-executed from every structural path that
    /// reaches them. Memory stays bounded through `DenseCache`'s `max_entries`
    /// cap, which recycles the oldest window of entries instead of growing.
    ///
    /// Nothing is currently marked no-cache; the bitset is kept so callers and
    /// the serialized surface stay stable.
    fn compute_no_cache(&mut self) {
        self.no_cache.clear();
        self.no_cache.resize(self.atoms.len(), false);
    }

    /// Serialize to JSON
    #[inline]
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON and optimize
    #[inline]
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        let mut grammar: Grammar = serde_json::from_str(s)?;
        grammar.optimize();
        grammar.compute_no_cache();
        Ok(grammar)
    }

    /// Analyze the grammar for optimization opportunities
    pub fn analyze(&self) -> GrammarAnalysis {
        let mut atom_types = HashMap::new();

        for atom in &self.atoms {
            let ty = match atom {
                Atom::Str { .. } => "str",
                Atom::Re { .. } => "re",
                Atom::Sequence { .. } => "sequence",
                Atom::Alternative { .. } => "alternative",
                Atom::Repetition { .. } => "repetition",
                Atom::Named { .. } => "named",
                Atom::Entity { .. } => "entity",
                Atom::Lookahead { .. } => "lookahead",
                Atom::Cut => "cut",
                Atom::Ignore { .. } => "ignore",
                Atom::Capture { .. } => "capture",
                Atom::Scope { .. } => "scope",
                Atom::Dynamic { .. } => "dynamic",
                Atom::Custom { .. } => "custom",
            };
            *atom_types.entry(ty).or_insert(0) += 1;
        }

        GrammarAnalysis {
            total_atoms: self.atoms.len(),
            atom_types,
            has_repetitions: self
                .atoms
                .iter()
                .any(|a| matches!(a, Atom::Repetition { .. })),
            has_lookaheads: self
                .atoms
                .iter()
                .any(|a| matches!(a, Atom::Lookahead { .. })),
            has_captures: self.atoms.iter().any(|a| matches!(a, Atom::Capture { .. })),
            has_scopes: self.atoms.iter().any(|a| matches!(a, Atom::Scope { .. })),
            has_dynamic: self.atoms.iter().any(|a| matches!(a, Atom::Dynamic { .. })),
        }
    }

    /// Analyze the grammar for potential issues and return warnings
    ///
    /// This method checks for:
    /// - Left recursion (causes infinite loops in PEG)
    /// - Unreachable alternatives
    /// - Unused atoms
    /// - Excessive backtracking potential
    /// - Empty composites (sequences/alternatives)
    /// - Useless repetitions
    /// - Infinite loops
    ///
    /// # Example
    ///
    /// ```
    /// use parsanol::portable::{Grammar, Atom};
    ///
    /// let mut grammar = Grammar::new();
    /// grammar.add_atom(Atom::Str { pattern: "hello".to_string() });
    /// grammar.root = 0;
    ///
    /// let warnings = grammar.analyze_warnings();
    /// for w in &warnings {
    ///     println!("{}", w);
    /// }
    /// ```
    pub fn analyze_warnings(&self) -> Vec<GrammarWarning> {
        GrammarAnalyzer::new(self).analyze()
    }

    /// One-shot parse convenience method
    ///
    /// Creates an arena and parser internally, parses the input, and returns the AST.
    /// This is the simplest way to parse input when you don't need fine-grained control.
    ///
    /// # Example
    ///
    /// ```
    /// use parsanol::portable::parser_dsl::{GrammarBuilder, str};
    ///
    /// let grammar = GrammarBuilder::new()
    ///     .rule("hello", str("hello"))
    ///     .build();
    ///
    /// let result = grammar.parse("hello");
    /// assert!(result.is_ok());
    /// ```
    pub fn parse(
        &self,
        input: &str,
    ) -> Result<crate::portable::ast::AstNode, crate::portable::ast::ParseError> {
        use crate::portable::arena::AstArena;
        use crate::portable::parser::PortableParser;

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(self, input, &mut arena);
        parser.parse()
    }

    /// Parse input and return the AST with end position
    ///
    /// This is similar to `parse()` but also returns the end position,
    /// which is useful for partial parsing or when you need to know
    /// how much input was consumed.
    ///
    /// # Arguments
    /// * `input` - The input string to parse
    ///
    /// # Returns
    /// * `Ok(ParseResult)` on success, containing the AST and end position
    /// * `Err(ParseError)` on failure
    pub fn parse_with_pos(
        &self,
        input: &str,
    ) -> Result<crate::portable::ast::ParseResult, crate::portable::ast::ParseError> {
        use crate::portable::arena::AstArena;
        use crate::portable::parser::PortableParser;

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(self, input, &mut arena);
        parser.parse_with_end_pos()
    }

    /// Parse multiple inputs in batch mode
    ///
    /// This method is optimized for parsing many inputs with the same grammar.
    /// It reuses internal buffers and provides better cache locality than
    /// calling `parse()` in a loop.
    ///
    /// # Arguments
    /// * `inputs` - Iterator of input strings to parse
    ///
    /// # Returns
    /// A vector of results, one for each input, in the same order.
    ///
    /// # Example
    ///
    /// ```
    /// use parsanol::portable::parser_dsl::{GrammarBuilder, str};
    ///
    /// let grammar = GrammarBuilder::new()
    ///     .rule("hello", str("hello"))
    ///     .build();
    ///
    /// let inputs = vec!["hello", "hello", "hello"];
    /// let results = grammar.parse_batch(inputs);
    ///
    /// assert_eq!(results.len(), 3);
    /// assert!(results.iter().all(|r| r.is_ok()));
    /// ```
    pub fn parse_batch<'a, I>(
        &self,
        inputs: I,
    ) -> Vec<Result<crate::portable::ast::AstNode, crate::portable::ast::ParseError>>
    where
        I: IntoIterator<Item = &'a str>,
    {
        use crate::portable::arena::AstArena;
        use crate::portable::parser::PortableParser;

        let inputs_vec: Vec<&'a str> = inputs.into_iter().collect();
        let mut results = Vec::with_capacity(inputs_vec.len());

        // Estimate total size for arena pre-allocation
        let total_size: usize = inputs_vec.iter().map(|s| s.len()).sum();
        let avg_size = if inputs_vec.is_empty() {
            0
        } else {
            total_size / inputs_vec.len()
        };

        // Create a reusable arena sized for the average input
        let mut arena = AstArena::for_input(avg_size.max(256));

        for input in inputs_vec {
            // Reset arena for each parse (keep strings for reuse)
            arena.reset();

            let mut parser = PortableParser::new(self, input, &mut arena);
            results.push(parser.parse());
        }

        results
    }

    /// Parse multiple inputs with a callback for each result
    ///
    /// This method is useful when you want to process results immediately
    /// rather than collecting them all into a vector.
    ///
    /// # Arguments
    /// * `inputs` - Iterator of input strings to parse
    /// * `callback` - Function called with each (index, input, result) tuple
    ///
    /// # Example
    ///
    /// ```
    /// use parsanol::portable::parser_dsl::{GrammarBuilder, str};
    ///
    /// let grammar = GrammarBuilder::new()
    ///     .rule("hello", str("hello"))
    ///     .build();
    ///
    /// let inputs = vec!["hello", "world"];
    /// let mut success_count = 0;
    ///
    /// grammar.parse_batch_with_callback(inputs, |idx, input, result| {
    ///     if result.is_ok() {
    ///         success_count += 1;
    ///     }
    /// });
    ///
    /// assert_eq!(success_count, 1); // Only "hello" matches
    /// ```
    pub fn parse_batch_with_callback<'a, I, F>(&self, inputs: I, mut callback: F)
    where
        I: IntoIterator<Item = &'a str>,
        F: FnMut(
            usize,
            &'a str,
            Result<crate::portable::ast::AstNode, crate::portable::ast::ParseError>,
        ),
    {
        use crate::portable::arena::AstArena;
        use crate::portable::parser::PortableParser;

        let mut arena = AstArena::new();

        for (idx, input) in inputs.into_iter().enumerate() {
            // Reset arena for each parse
            arena.reset();

            let mut parser = PortableParser::new(self, input, &mut arena);
            let result = parser.parse();

            callback(idx, input, result);
        }
    }
}

impl Default for Grammar {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Grammar Optimization
// ============================================================================

impl Grammar {
    /// Optimize the grammar by merging adjacent literal atoms in sequences.
    ///
    /// This pass reduces atom count by ~30-40% for typical grammars:
    /// - Adjacent `Re` atoms are merged into a single `Re` atom
    ///   (e.g., `[Ee][Nn][Tt][Ii][Tt][Yy]` → one atom)
    /// - Adjacent `Str` atoms are merged into a single `Str` atom
    ///   (e.g., `"ab"` + `"cd"` → `"abcd"`)
    ///
    /// Fewer atoms means:
    /// - Smaller cache (fewer atom_id × position slots)
    /// - Fewer arena allocations during parsing
    /// - Less memory overall
    pub fn optimize(&mut self) {
        // Phase 1: For each Sequence, merge adjacent Re/Str runs. The merged
        // pattern is appended as a NEW atom and only this sequence's children
        // are rewritten to point at it — the original atoms are never
        // mutated, because they may be shared with other sequences (a
        // mutated shared atom silently changes what those sequences match).
        //
        // After this pass, the run's original atoms may be unreferenced.
        // Phase 2 compacts them away.

        let initial_len = self.atoms.len();
        for seq_idx in 0..initial_len {
            if let Atom::Sequence { atoms: children } = &self.atoms[seq_idx] {
                // Find runs and plan the merge
                let mut new_children: Vec<usize> = Vec::with_capacity(children.len());
                let mut merges: Vec<(String, bool)> = Vec::new();
                // (merged_pattern, is_re); scan order matches placeholder order
                let mut i = 0;
                let children = children.clone();

                while i < children.len() {
                    let idx = children[i];

                    if matches!(self.atoms.get(idx), Some(Atom::Re { .. })) {
                        // Re runs stay UNMERGED: concatenating regex sources
                        // is not language-preserving ("a|" + "b" = "a|b"),
                        // and a merged run could accept tokens the original
                        // sequence would reject (issue parsanol-ruby#39:
                        // H2/_2O accepted under native, rejected by parslet).
                        new_children.push(idx);
                        i += 1;
                    } else if matches!(self.atoms.get(idx), Some(Atom::Str { .. })) {
                        let run_start = i;
                        while i < children.len()
                            && matches!(self.atoms.get(children[i]), Some(Atom::Str { .. }))
                        {
                            i += 1;
                        }

                        if i - run_start >= 2 {
                            let mut merged = String::new();
                            for child in &children[run_start..i] {
                                if let Atom::Str { pattern } = &self.atoms[*child] {
                                    merged.push_str(pattern);
                                }
                            }
                            merges.push((merged, false));
                            new_children.push(usize::MAX);
                        } else {
                            new_children.push(idx);
                        }
                    } else {
                        new_children.push(idx);
                        i += 1;
                    }
                }

                if !merges.is_empty() {
                    // Append each merged pattern as a fresh atom, replacing
                    // its placeholder in scan order; the run's original atoms
                    // fall out of this sequence and the compaction pass
                    // removes them when nothing else references them.
                    let mut next_merge = 0;
                    for slot in new_children.iter_mut() {
                        if *slot == usize::MAX {
                            let (pattern, is_re) = &merges[next_merge];
                            next_merge += 1;
                            let new_idx = self.atoms.len();
                            if *is_re {
                                self.atoms.push(Atom::Re {
                                    pattern: pattern.clone(),
                                });
                            } else {
                                self.atoms.push(Atom::Str {
                                    pattern: pattern.clone(),
                                });
                            }
                            *slot = new_idx;
                        }
                    }
                    // Update sequence children
                    if let Atom::Sequence { atoms } = &mut self.atoms[seq_idx] {
                        *atoms = new_children;
                    }
                }
            }
        }

        // Phase 2: Compact — remove unreferenced atoms and remap indices.
        self.compact_atoms();
    }

    /// Remove unreferenced atoms from the grammar and remap all indices.
    ///
    /// After merging, some atoms are no longer referenced by any sequence.
    /// This pass:
    /// 1. Marks all reachable atoms (BFS from root)
    /// 2. Builds a remap table (old_idx → new_idx)
    /// 3. Rewrites all atom references using the remap table
    /// 4. Removes dead atoms from self.atoms
    fn compact_atoms(&mut self) {
        let n = self.atoms.len();

        // Step 1: Mark reachable atoms
        let mut reachable = vec![false; n];
        let mut queue = std::collections::VecDeque::new();
        reachable[self.root] = true;
        queue.push_back(self.root);

        while let Some(idx) = queue.pop_front() {
            let children: Vec<usize> = match &self.atoms[idx] {
                Atom::Sequence { atoms } => atoms.clone(),
                Atom::Alternative { atoms } => atoms.clone(),
                Atom::Repetition { atom, .. } => vec![*atom],
                Atom::Named { atom, .. } => vec![*atom],
                Atom::Entity { atom } => vec![*atom],
                Atom::Lookahead { atom, .. } => vec![*atom],
                Atom::Ignore { atom } => vec![*atom],
                Atom::Capture { atom, .. } => vec![*atom],
                Atom::Scope { atom } => vec![*atom],
                _ => vec![],
            };
            for child in children {
                if child < n && !reachable[child] {
                    reachable[child] = true;
                    queue.push_back(child);
                }
            }
        }

        // Step 2: Build remap table
        let mut remap = vec![0usize; n];
        let mut new_idx = 0;
        for old_idx in 0..n {
            if reachable[old_idx] {
                remap[old_idx] = new_idx;
                new_idx += 1;
            }
        }

        // If everything is reachable, no compaction needed
        if new_idx == n {
            return;
        }

        // Step 3: Rewrite all references
        for atom in &mut self.atoms {
            match atom {
                Atom::Sequence { atoms } => {
                    for idx in atoms.iter_mut() {
                        *idx = remap[*idx];
                    }
                }
                Atom::Alternative { atoms } => {
                    for idx in atoms.iter_mut() {
                        *idx = remap[*idx];
                    }
                }
                Atom::Repetition { atom, .. } => {
                    *atom = remap[*atom];
                }
                Atom::Named { atom, .. } => {
                    *atom = remap[*atom];
                }
                Atom::Entity { atom } => {
                    *atom = remap[*atom];
                }
                Atom::Lookahead { atom, .. } => {
                    *atom = remap[*atom];
                }
                Atom::Ignore { atom } => {
                    *atom = remap[*atom];
                }
                Atom::Capture { atom, .. } => {
                    *atom = remap[*atom];
                }
                Atom::Scope { atom } => {
                    *atom = remap[*atom];
                }
                _ => {}
            }
        }

        // Update root
        self.root = remap[self.root];

        // Step 4: Remove dead atoms
        let mut write = 0;
        for (read, is_reachable) in reachable.iter().enumerate() {
            if *is_reachable {
                self.atoms.swap(write, read);
                write += 1;
            }
        }
        self.atoms.truncate(write);
    }
}

/// Result of grammar analysis
pub struct GrammarAnalysis {
    /// Total number of atoms
    pub total_atoms: usize,

    /// Count by type
    pub atom_types: HashMap<&'static str, usize>,

    /// Whether grammar contains repetitions
    pub has_repetitions: bool,

    /// Whether grammar contains lookaheads
    pub has_lookaheads: bool,

    /// Whether grammar contains captures
    pub has_captures: bool,

    /// Whether grammar contains scopes
    pub has_scopes: bool,

    /// Whether grammar contains dynamic atoms
    pub has_dynamic: bool,
}

// ============================================================================
// AtomVisitor Trait
// ============================================================================

/// Visitor trait for walking over all Atom variants in a grammar
///
/// This trait provides a way to visit each atom type in a grammar,
/// useful for grammar analysis, transformation, and validation.
///
/// # Example
///
/// ```rust,ignore
/// use parsanol::portable::grammar::{AtomVisitor, Atom, Grammar};
///
/// struct AtomCounter {
///     str_count: usize,
///     re_count: usize,
/// }
///
/// impl AtomVisitor for AtomCounter {
///     fn visit_str(&mut self, _pattern: &str) {
///         self.str_count += 1;
///     }
///
///     fn visit_re(&mut self, _pattern: &str) {
///         self.re_count += 1;
///     }
/// }
///
/// let grammar = Grammar::new();
/// let mut counter = AtomCounter { str_count: 0, re_count: 0 };
/// grammar.visit_atoms(&mut counter);
/// ```
pub trait AtomVisitor {
    /// Visit a string atom
    fn visit_str(&mut self, _pattern: &str) {}

    /// Visit a regex atom
    fn visit_re(&mut self, _pattern: &str) {}

    /// Visit a sequence atom (called before visiting children)
    fn visit_sequence_pre(&mut self, _atoms: &[usize]) {}

    /// Visit a sequence atom (called after visiting children)
    fn visit_sequence_post(&mut self, _atoms: &[usize]) {}

    /// Visit an alternative atom (called before visiting children)
    fn visit_alternative_pre(&mut self, _atoms: &[usize]) {}

    /// Visit an alternative atom (called after visiting children)
    fn visit_alternative_post(&mut self, _atoms: &[usize]) {}

    /// Visit a repetition atom (called before visiting child)
    fn visit_repetition_pre(&mut self, _atom: usize, _min: usize, _max: Option<usize>) {}

    /// Visit a repetition atom (called after visiting child)
    fn visit_repetition_post(&mut self, _atom: usize, _min: usize, _max: Option<usize>) {}

    /// Visit a named atom (called before visiting child)
    fn visit_named_pre(&mut self, _name: &str, _atom: usize) {}

    /// Visit a named atom (called after visiting child)
    fn visit_named_post(&mut self, _name: &str, _atom: usize) {}

    /// Visit an entity reference
    fn visit_entity(&mut self, _atom: usize) {}

    /// Visit a lookahead atom (called before visiting child)
    fn visit_lookahead_pre(&mut self, _atom: usize, _positive: bool) {}

    /// Visit a lookahead atom (called after visiting child)
    fn visit_lookahead_post(&mut self, _atom: usize, _positive: bool) {}

    /// Visit a cut atom
    fn visit_cut(&mut self) {}

    /// Visit an ignore atom (called before visiting child)
    fn visit_ignore_pre(&mut self, _atom: usize) {}

    /// Visit an ignore atom (called after visiting child)
    fn visit_ignore_post(&mut self, _atom: usize) {}

    /// Visit a capture atom (called before visiting child)
    fn visit_capture_pre(&mut self, _name: &str, _atom: usize) {}

    /// Visit a capture atom (called after visiting child)
    fn visit_capture_post(&mut self, _name: &str, _atom: usize) {}

    /// Visit a scope atom (called before visiting child)
    fn visit_scope_pre(&mut self, _atom: usize) {}

    /// Visit a scope atom (called after visiting child)
    fn visit_scope_post(&mut self, _atom: usize) {}

    /// Visit a dynamic atom
    fn visit_dynamic(&mut self, _callback_id: u64) {}

    /// Visit a custom atom
    fn visit_custom(&mut self, _id: u64) {}
}

impl Grammar {
    /// Visit all atoms in this grammar using the provided visitor
    ///
    /// Traverses atoms starting from the root atom, visiting each atom
    /// in depth-first order.
    pub fn visit_atoms<V: AtomVisitor>(&self, visitor: &mut V) {
        self.visit_atom(self.root, visitor);
    }

    /// Visit a specific atom and its children
    fn visit_atom<V: AtomVisitor>(&self, idx: usize, visitor: &mut V) {
        if let Some(atom) = self.atoms.get(idx) {
            match atom {
                Atom::Str { pattern } => {
                    visitor.visit_str(pattern);
                }
                Atom::Re { pattern } => {
                    visitor.visit_re(pattern);
                }
                Atom::Sequence { atoms } => {
                    visitor.visit_sequence_pre(atoms);
                    for &child_idx in atoms {
                        self.visit_atom(child_idx, visitor);
                    }
                    visitor.visit_sequence_post(atoms);
                }
                Atom::Alternative { atoms } => {
                    visitor.visit_alternative_pre(atoms);
                    for &child_idx in atoms {
                        self.visit_atom(child_idx, visitor);
                    }
                    visitor.visit_alternative_post(atoms);
                }
                Atom::Repetition { atom, min, max, .. } => {
                    visitor.visit_repetition_pre(*atom, *min, *max);
                    self.visit_atom(*atom, visitor);
                    visitor.visit_repetition_post(*atom, *min, *max);
                }
                Atom::Named { name, atom } => {
                    visitor.visit_named_pre(name, *atom);
                    self.visit_atom(*atom, visitor);
                    visitor.visit_named_post(name, *atom);
                }
                Atom::Entity { atom } => {
                    visitor.visit_entity(*atom);
                    // Note: We don't recursively visit entity targets to avoid infinite loops
                    // If you need to visit all reachable atoms, use visit_atoms_reachable instead
                }
                Atom::Lookahead { atom, positive } => {
                    visitor.visit_lookahead_pre(*atom, *positive);
                    self.visit_atom(*atom, visitor);
                    visitor.visit_lookahead_post(*atom, *positive);
                }
                Atom::Cut => {
                    visitor.visit_cut();
                }
                Atom::Ignore { atom } => {
                    visitor.visit_ignore_pre(*atom);
                    self.visit_atom(*atom, visitor);
                    visitor.visit_ignore_post(*atom);
                }
                Atom::Capture { name, atom } => {
                    visitor.visit_capture_pre(name, *atom);
                    self.visit_atom(*atom, visitor);
                    visitor.visit_capture_post(name, *atom);
                }
                Atom::Scope { atom } => {
                    visitor.visit_scope_pre(*atom);
                    self.visit_atom(*atom, visitor);
                    visitor.visit_scope_post(*atom);
                }
                Atom::Dynamic { callback_id } => {
                    visitor.visit_dynamic(*callback_id);
                }
                Atom::Custom { id } => {
                    visitor.visit_custom(*id);
                }
            }
        }
    }
}

/// Default implementation for visiting atoms - counts atom types
#[derive(Debug, Clone, Default)]
pub struct AtomTypeCounter {
    /// Count of string atoms
    pub str_count: usize,
    /// Count of regex atoms
    pub re_count: usize,
    /// Count of sequence atoms
    pub sequence_count: usize,
    /// Count of alternative atoms
    pub alternative_count: usize,
    /// Count of repetition atoms
    pub repetition_count: usize,
    /// Count of named atoms
    pub named_count: usize,
    /// Count of entity atoms
    pub entity_count: usize,
    /// Count of lookahead atoms
    pub lookahead_count: usize,
    /// Count of cut atoms
    pub cut_count: usize,
    /// Count of ignore atoms
    pub ignore_count: usize,
    /// Count of capture atoms
    pub capture_count: usize,
    /// Count of scope atoms
    pub scope_count: usize,
    /// Count of dynamic atoms
    pub dynamic_count: usize,
    /// Count of custom atoms
    pub custom_count: usize,
}

impl AtomVisitor for AtomTypeCounter {
    fn visit_str(&mut self, _pattern: &str) {
        self.str_count += 1;
    }

    fn visit_re(&mut self, _pattern: &str) {
        self.re_count += 1;
    }

    fn visit_sequence_pre(&mut self, _atoms: &[usize]) {
        self.sequence_count += 1;
    }

    fn visit_alternative_pre(&mut self, _atoms: &[usize]) {
        self.alternative_count += 1;
    }

    fn visit_repetition_pre(&mut self, _atom: usize, _min: usize, _max: Option<usize>) {
        self.repetition_count += 1;
    }

    fn visit_named_pre(&mut self, _name: &str, _atom: usize) {
        self.named_count += 1;
    }

    fn visit_entity(&mut self, _atom: usize) {
        self.entity_count += 1;
    }

    fn visit_lookahead_pre(&mut self, _atom: usize, _positive: bool) {
        self.lookahead_count += 1;
    }

    fn visit_cut(&mut self) {
        self.cut_count += 1;
    }

    fn visit_ignore_pre(&mut self, _atom: usize) {
        self.ignore_count += 1;
    }

    fn visit_capture_pre(&mut self, _name: &str, _atom: usize) {
        self.capture_count += 1;
    }

    fn visit_scope_pre(&mut self, _atom: usize) {
        self.scope_count += 1;
    }

    fn visit_dynamic(&mut self, _callback_id: u64) {
        self.dynamic_count += 1;
    }

    fn visit_custom(&mut self, _id: u64) {
        self.custom_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::parser_dsl::{choice, dynamic, re, seq, str, GrammarBuilder};

    #[test]
    fn test_from_json_memoizes_every_atom_kind() {
        // The JSON registration path (used by the Ruby FFI) must not exempt
        // any atom kind from packrat memoization: exempting them re-executes
        // non-terminal subtrees from every structural path and collapses
        // large-input performance (parsanol-ruby#52 regression).
        let grammar = GrammarBuilder::new()
            .rule(
                "root",
                choice(vec![
                    dynamic(seq(vec![dynamic(str("a")), dynamic(str("b"))])),
                    dynamic(re("[0-9]+")),
                ]),
            )
            .build();

        let round_tripped = Grammar::from_json(&grammar.to_json().unwrap()).unwrap();

        assert!(
            round_tripped.no_cache.iter().all(|&nc| !nc),
            "no atom may be marked no-cache after registration"
        );
        assert_eq!(
            round_tripped.cacheable_atom_count(),
            round_tripped.atom_count()
        );
    }

    #[test]
    fn test_optimize_does_not_mutate_shared_atoms() {
        // Str("ab") is shared by two sequences, each with a different
        // adjacent Str. Merging one run must not change what the other
        // sequence matches.
        let mut grammar = Grammar::new();
        let ab = grammar.add_atom(Atom::Str {
            pattern: "ab".to_string(),
        });
        let c = grammar.add_atom(Atom::Str {
            pattern: "c".to_string(),
        });
        let seq1 = grammar.add_atom(Atom::Sequence { atoms: vec![ab, c] });
        let d = grammar.add_atom(Atom::Str {
            pattern: "d".to_string(),
        });
        let seq2 = grammar.add_atom(Atom::Sequence { atoms: vec![ab, d] });
        let root = grammar.add_atom(Atom::Alternative {
            atoms: vec![seq1, seq2],
        });
        grammar.root = root;

        grammar.optimize();

        let seq1 = match grammar.get_atom(grammar.root) {
            Some(Atom::Alternative { atoms }) => match grammar.get_atom(atoms[0]) {
                Some(Atom::Sequence { atoms }) => atoms.clone(),
                other => panic!("branch 0 is not a sequence: {other:?}"),
            },
            other => panic!("root is not an alternative: {other:?}"),
        };
        let seq2 = match grammar.get_atom(grammar.root) {
            Some(Atom::Alternative { atoms }) => match grammar.get_atom(atoms[1]) {
                Some(Atom::Sequence { atoms }) => atoms.clone(),
                other => panic!("branch 1 is not a sequence: {other:?}"),
            },
            other => panic!("root is not an alternative: {other:?}"),
        };
        let branch_pattern = |children: &Vec<usize>| match grammar.get_atom(children[0]) {
            Some(Atom::Str { pattern }) => pattern.clone(),
            other => panic!("first child is not a Str: {other:?}"),
        };
        assert_eq!(branch_pattern(&seq1), "abc");
        assert_eq!(branch_pattern(&seq2), "abd");
    }

    #[test]
    fn test_grammar_new() {
        let grammar = Grammar::new();
        assert_eq!(grammar.atom_count(), 0);
    }

    #[test]
    fn test_grammar_add_atom() {
        let mut grammar = Grammar::new();

        let idx = grammar.add_atom(Atom::Str {
            pattern: "hello".to_string(),
        });

        assert_eq!(idx, 0);
        assert_eq!(grammar.atom_count(), 1);

        let atom = grammar.get_atom(0).unwrap();
        match atom {
            Atom::Str { pattern } => assert_eq!(pattern, "hello"),
            _ => panic!("Wrong atom type"),
        }
    }

    #[test]
    fn test_grammar_json_roundtrip() {
        let mut grammar = Grammar::new();

        grammar.add_atom(Atom::Str {
            pattern: "hello".to_string(),
        });
        grammar.add_atom(Atom::Sequence { atoms: vec![0] });

        let json = grammar.to_json().unwrap();
        let parsed = Grammar::from_json(&json).unwrap();

        // After optimize(), unreferenced atoms are compacted away.
        // The Sequence at index 1 is unreachable (root=0), so only 1 atom remains.
        assert_eq!(parsed.atom_count(), 1);
    }

    #[test]
    fn test_grammar_analyze() {
        let mut grammar = Grammar::new();

        grammar.add_atom(Atom::Str {
            pattern: "hello".to_string(),
        });
        grammar.add_atom(Atom::Repetition {
            atom: 0,
            min: 0,
            max: Some(100),
            tag: RepetitionTag::Repetition,
        });

        let analysis = grammar.analyze();

        assert_eq!(analysis.total_atoms, 2);
        assert!(analysis.has_repetitions);
        assert!(!analysis.has_lookaheads);
    }
}

#[cfg(test)]
mod tag_tests {
    use super::*;

    #[test]
    fn test_repetition_tag_deserialize() {
        let json = r#"{"atoms":[{"Str":{"pattern":"a"}},{"Repetition":{"atom":0,"min":0,"max":1,"tag":"Maybe"}}],"root":1}"#;
        let g = Grammar::from_json(json).unwrap();
        match g.get_atom(1) {
            Some(Atom::Repetition { tag, .. }) => {
                assert_eq!(*tag, RepetitionTag::Maybe);
            }
            other => panic!("expected Repetition, got {:?}", other.map(|_| ())),
        }
    }
}
