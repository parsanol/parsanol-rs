//! Grammar types for Parsanol
//!
//! This module defines the in-memory representation of parsed grammars.
//! Grammars are serialized to JSON from Ruby and deserialized here.

use crate::portable::grammar_analysis::{GrammarAnalyzer, GrammarWarning};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

impl Grammar {
    /// Create a new empty grammar
    #[inline]
    pub fn new() -> Self {
        Self {
            atoms: Vec::new(),
            root: 0,
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

    /// Serialize to JSON
    #[inline]
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON
    #[inline]
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
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
                Atom::Repetition { atom, min, max } => {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(parsed.atom_count(), 2);
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
        });

        let analysis = grammar.analyze();

        assert_eq!(analysis.total_atoms, 2);
        assert!(analysis.has_repetitions);
        assert!(!analysis.has_lookaheads);
    }
}
