//! Grammar analysis for backend selection
//!
//! This module provides tools for analyzing grammar characteristics
//! to help select the most appropriate parsing backend.

use crate::portable::grammar::{Atom, Grammar};

use super::Backend;

/// Check if grammar has nested repetitions
///
/// Nested repetitions (e.g., `(a*)*`) cause exponential backtracking O(2^n)
/// in the bytecode VM, while Packrat guarantees O(n).
pub fn has_nested_repetition(grammar: &Grammar) -> bool {
    for atom in &grammar.atoms {
        if let Atom::Repetition { atom: inner_idx, .. } = atom {
            if let Some(inner) = grammar.get_atom(*inner_idx) {
                if matches!(inner, Atom::Repetition { .. }) {
                    return true;
                }
            }
        }
    }
    false
}

/// Grammar analysis result for backend selection
#[derive(Debug, Clone)]
pub struct GrammarAnalysis {
    /// Number of atoms in the grammar
    pub atom_count: usize,

    /// Whether the grammar has nested repetitions (e.g., `(a*)*`)
    /// This is the ONLY criterion for Packrat vs Bytecode selection.
    /// Nested repetitions cause exponential backtracking O(2^n) in bytecode VM,
    /// while Packrat guarantees O(n).
    pub has_nested_repetition: bool,
}

impl GrammarAnalysis {
    /// Analyze a grammar for backend selection
    pub fn analyze(grammar: &Grammar) -> Self {
        Self {
            atom_count: grammar.atoms.len(),
            has_nested_repetition: has_nested_repetition(grammar),
        }
    }

    /// Recommend a backend based on analysis
    /// Hard rule: nested repetitions → Packrat, otherwise → Bytecode
    pub fn recommended_backend(&self) -> Backend {
        if self.has_nested_repetition {
            Backend::Packrat
        } else {
            Backend::Bytecode
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::grammar::Grammar;

    fn simple_grammar() -> Grammar {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "hello".to_string(),
        });
        grammar.root = 0;
        grammar
    }

    fn nested_repetition_grammar() -> Grammar {
        let mut grammar = Grammar::new();
        let a = grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        let inner = grammar.add_atom(Atom::Repetition {
            atom: a,
            min: 0,
            max: None,
        });
        let outer = grammar.add_atom(Atom::Repetition {
            atom: inner,
            min: 0,
            max: None,
        });
        grammar.root = outer;
        grammar
    }

    #[test]
    fn test_simple_grammar_no_nested_repetition() {
        let grammar = simple_grammar();
        assert!(!has_nested_repetition(&grammar));

        let analysis = GrammarAnalysis::analyze(&grammar);
        assert!(!analysis.has_nested_repetition);
        assert_eq!(analysis.recommended_backend(), Backend::Bytecode);
    }

    #[test]
    fn test_nested_repetition_detected() {
        let grammar = nested_repetition_grammar();
        assert!(has_nested_repetition(&grammar));

        let analysis = GrammarAnalysis::analyze(&grammar);
        assert!(analysis.has_nested_repetition);
        assert_eq!(analysis.recommended_backend(), Backend::Packrat);
    }
}
