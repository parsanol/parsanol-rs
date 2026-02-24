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
    pub fn parse(&self, input: &str) -> Result<crate::portable::ast::AstNode, crate::portable::ast::ParseError> {
        use crate::portable::arena::AstArena;
        use crate::portable::parser::PortableParser;

        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(self, input, &mut arena);
        parser.parse()
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
