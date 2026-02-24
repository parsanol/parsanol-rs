//! Grammar analysis and warnings for Parsanol
//!
//! This module provides grammar analysis that warns about:
//! - Left recursion (causes infinite loops in PEG)
//! - Unreachable alternatives
//! - Unused atoms
//! - Excessive backtracking potential
//!
//! # Example
//!
//! ```
//! use parsanol::portable::{Grammar, Atom, GrammarAnalyzer, WarningKind};
//!
//! let mut grammar = Grammar::new();
//! grammar.add_atom(Atom::Entity { atom: 0 }); // Left recursive!
//! grammar.root = 0;
//!
//! let warnings = GrammarAnalyzer::new(&grammar).analyze();
//! for warning in &warnings {
//!     println!("{:?}: {}", warning.kind, warning.message);
//! }
//! ```

use crate::portable::grammar::{Atom, Grammar};
use std::collections::{HashMap, HashSet};

/// Kind of grammar warning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningKind {
    /// Direct or indirect left recursion
    ///
    /// PEG parsers cannot handle left recursion and will loop infinitely.
    /// Example: `expr = expr "+" term | term`
    LeftRecursion,

    /// An alternative can never match because earlier alternatives always match first
    ///
    /// Example: `str("a") | str("a")` - second "a" is unreachable
    UnreachableAlternative,

    /// An atom is defined but never referenced
    ///
    /// This may indicate dead code or a typo in the grammar.
    UnusedAtom,

    /// Potentially excessive backtracking
    ///
    /// This can occur with nested alternatives and repetitions that
    /// can match the same input in multiple ways.
    ExcessiveBacktracking,

    /// Empty sequence or alternative
    ///
    /// An empty sequence always matches. An empty alternative never matches.
    EmptyComposite,

    /// Repetition with min=0 and max=0 (always matches nothing)
    UselessRepetition,

    /// Self-referential Entity without base case
    ///
    /// An Entity that only references itself with no termination.
    InfiniteLoop,
}

impl std::fmt::Display for WarningKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LeftRecursion => write!(f, "left recursion"),
            Self::UnreachableAlternative => write!(f, "unreachable alternative"),
            Self::UnusedAtom => write!(f, "unused atom"),
            Self::ExcessiveBacktracking => write!(f, "excessive backtracking"),
            Self::EmptyComposite => write!(f, "empty composite"),
            Self::UselessRepetition => write!(f, "useless repetition"),
            Self::InfiniteLoop => write!(f, "infinite loop"),
        }
    }
}

/// A grammar warning
#[derive(Debug, Clone)]
pub struct GrammarWarning {
    /// The kind of warning
    pub kind: WarningKind,
    /// The atom ID where the warning was detected
    pub atom_id: usize,
    /// Human-readable message
    pub message: String,
    /// Related atom IDs (e.g., for left recursion chains)
    pub related_atoms: Vec<usize>,
}

impl GrammarWarning {
    /// Create a new warning
    pub fn new(kind: WarningKind, atom_id: usize, message: impl Into<String>) -> Self {
        Self {
            kind,
            atom_id,
            message: message.into(),
            related_atoms: Vec::new(),
        }
    }

    /// Add related atoms to the warning
    pub fn with_related(mut self, atoms: Vec<usize>) -> Self {
        self.related_atoms = atoms;
        self
    }
}

impl std::fmt::Display for GrammarWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[atom {}] {}: {}", self.atom_id, self.kind, self.message)?;
        if !self.related_atoms.is_empty() {
            write!(f, " (related atoms: {:?})", self.related_atoms)?;
        }
        Ok(())
    }
}

/// Grammar analyzer
pub struct GrammarAnalyzer<'a> {
    grammar: &'a Grammar,
    /// Cache of nullable atoms (can match empty string)
    nullable: HashMap<usize, bool>,
}

impl<'a> GrammarAnalyzer<'a> {
    /// Create a new analyzer for the given grammar
    pub fn new(grammar: &'a Grammar) -> Self {
        Self {
            grammar,
            nullable: HashMap::new(),
        }
    }

    /// Analyze the grammar and return all warnings
    pub fn analyze(&mut self) -> Vec<GrammarWarning> {
        let mut warnings = Vec::new();

        // Run all analyses
        self.detect_left_recursion(&mut warnings);
        self.detect_unused_atoms(&mut warnings);
        self.detect_empty_composites(&mut warnings);
        self.detect_useless_repetitions(&mut warnings);
        self.detect_infinite_loops(&mut warnings);
        self.detect_unreachable_alternatives(&mut warnings);
        self.detect_excessive_backtracking(&mut warnings);

        warnings
    }

    /// Detect left recursion (direct and indirect)
    ///
    /// Left recursion occurs when an atom can match by first matching itself.
    /// PEG parsers cannot handle this and will loop infinitely.
    fn detect_left_recursion(&mut self, warnings: &mut Vec<GrammarWarning>) {
        for atom_id in 0..self.grammar.atoms.len() {
            if let Some(chain) =
                self.find_left_recursive_path(atom_id, atom_id, &mut HashSet::new())
            {
                warnings.push(
                    GrammarWarning::new(
                        WarningKind::LeftRecursion,
                        atom_id,
                        format!(
                            "Atom {} is left-recursive (can match itself without consuming input)",
                            atom_id
                        ),
                    )
                    .with_related(chain),
                );
            }
        }
    }

    /// Find a left-recursive path from start_atom back to target_atom
    fn find_left_recursive_path(
        &mut self,
        start_atom: usize,
        target_atom: usize,
        visited: &mut HashSet<usize>,
    ) -> Option<Vec<usize>> {
        if visited.contains(&start_atom) {
            return None;
        }
        visited.insert(start_atom);

        let atom = self.grammar.get_atom(start_atom)?;

        match atom {
            Atom::Entity { atom } => {
                if *atom == target_atom {
                    Some(vec![start_atom, *atom])
                } else if !visited.contains(atom) {
                    self.find_left_recursive_path(*atom, target_atom, visited)
                        .map(|mut path| {
                            path.insert(0, start_atom);
                            path
                        })
                } else {
                    None
                }
            }
            Atom::Sequence { atoms } => {
                // Check first non-nullable atom in sequence
                for &child in atoms {
                    if child == target_atom && self.all_nullable_before(atoms, child) {
                        return Some(vec![start_atom, child]);
                    }
                    if let Some(mut path) =
                        self.find_left_recursive_path(child, target_atom, visited)
                    {
                        path.insert(0, start_atom);
                        return Some(path);
                    }
                    // If this child is not nullable, stop checking sequence
                    if !self.is_nullable(child) {
                        break;
                    }
                }
                None
            }
            Atom::Alternative { atoms } => {
                for &child in atoms {
                    if child == target_atom {
                        return Some(vec![start_atom, child]);
                    }
                    if let Some(mut path) =
                        self.find_left_recursive_path(child, target_atom, visited)
                    {
                        path.insert(0, start_atom);
                        return Some(path);
                    }
                }
                None
            }
            Atom::Named { atom, .. } | Atom::Ignore { atom } | Atom::Lookahead { atom, .. } => {
                if *atom == target_atom {
                    Some(vec![start_atom, *atom])
                } else if !visited.contains(atom) {
                    self.find_left_recursive_path(*atom, target_atom, visited)
                        .map(|mut path| {
                            path.insert(0, start_atom);
                            path
                        })
                } else {
                    None
                }
            }
            Atom::Repetition { atom, min, .. } => {
                if *min > 0 {
                    if *atom == target_atom {
                        return Some(vec![start_atom, *atom]);
                    }
                    if !visited.contains(atom) {
                        self.find_left_recursive_path(*atom, target_atom, visited)
                            .map(|mut path| {
                                path.insert(0, start_atom);
                                path
                            })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Atom::Str { .. } | Atom::Re { .. } | Atom::Cut => None,
        }
    }

    /// Check if all atoms before the target in a sequence are nullable
    fn all_nullable_before(&mut self, atoms: &[usize], target: usize) -> bool {
        for &atom in atoms {
            if atom == target {
                return true;
            }
            if !self.is_nullable(atom) {
                return false;
            }
        }
        true
    }

    /// Check if an atom is nullable (can match empty string)
    fn is_nullable(&mut self, atom_id: usize) -> bool {
        if let Some(&result) = self.nullable.get(&atom_id) {
            return result;
        }

        let result = self.compute_nullable(atom_id);
        self.nullable.insert(atom_id, result);
        result
    }

    /// Compute whether an atom is nullable
    fn compute_nullable(&mut self, atom_id: usize) -> bool {
        let Some(atom) = self.grammar.get_atom(atom_id) else {
            return false;
        };

        match atom {
            Atom::Str { pattern } => pattern.is_empty(),
            Atom::Re { .. } => false, // Assume regex requires at least one char
            Atom::Sequence { atoms } => atoms.iter().all(|&a| self.is_nullable(a)),
            Atom::Alternative { atoms } => atoms.iter().any(|&a| self.is_nullable(a)),
            Atom::Repetition { min, .. } => *min == 0,
            Atom::Named { atom, .. }
            | Atom::Entity { atom }
            | Atom::Ignore { atom }
            | Atom::Lookahead { atom, .. } => self.is_nullable(*atom),
            Atom::Cut => false,
        }
    }

    /// Detect atoms that are never referenced from the root
    fn detect_unused_atoms(&self, warnings: &mut Vec<GrammarWarning>) {
        let mut reachable = HashSet::new();
        self.collect_reachable(self.grammar.root, &mut reachable);

        for atom_id in 0..self.grammar.atoms.len() {
            if !reachable.contains(&atom_id) && atom_id != self.grammar.root {
                warnings.push(GrammarWarning::new(
                    WarningKind::UnusedAtom,
                    atom_id,
                    format!("Atom {} is defined but never reachable from root", atom_id),
                ));
            }
        }
    }

    /// Collect all atoms reachable from the given atom
    fn collect_reachable(&self, atom_id: usize, reachable: &mut HashSet<usize>) {
        if reachable.contains(&atom_id) {
            return;
        }
        reachable.insert(atom_id);

        let Some(atom) = self.grammar.get_atom(atom_id) else {
            return;
        };

        match atom {
            Atom::Str { .. } | Atom::Re { .. } | Atom::Cut => {}
            Atom::Sequence { atoms } | Atom::Alternative { atoms } => {
                for &child in atoms {
                    self.collect_reachable(child, reachable);
                }
            }
            Atom::Repetition { atom, .. }
            | Atom::Named { atom, .. }
            | Atom::Entity { atom }
            | Atom::Ignore { atom }
            | Atom::Lookahead { atom, .. } => {
                self.collect_reachable(*atom, reachable);
            }
        }
    }

    /// Detect empty sequences and alternatives
    fn detect_empty_composites(&self, warnings: &mut Vec<GrammarWarning>) {
        for (atom_id, atom) in self.grammar.atoms.iter().enumerate() {
            match atom {
                Atom::Sequence { atoms } if atoms.is_empty() => {
                    warnings.push(GrammarWarning::new(
                        WarningKind::EmptyComposite,
                        atom_id,
                        "Empty sequence always matches (matches empty string)",
                    ));
                }
                Atom::Alternative { atoms } if atoms.is_empty() => {
                    warnings.push(GrammarWarning::new(
                        WarningKind::EmptyComposite,
                        atom_id,
                        "Empty alternative never matches",
                    ));
                }
                _ => {}
            }
        }
    }

    /// Detect useless repetitions (min=0, max=0)
    fn detect_useless_repetitions(&self, warnings: &mut Vec<GrammarWarning>) {
        for (atom_id, atom) in self.grammar.atoms.iter().enumerate() {
            if let Atom::Repetition {
                min: 0,
                max: Some(0),
                ..
            } = atom
            {
                warnings.push(GrammarWarning::new(
                    WarningKind::UselessRepetition,
                    atom_id,
                    "Repetition with min=0 and max=0 always matches nothing",
                ));
            }
        }
    }

    /// Detect infinite loops (Entity that only references itself)
    fn detect_infinite_loops(&mut self, warnings: &mut Vec<GrammarWarning>) {
        for atom_id in 0..self.grammar.atoms.len() {
            if let Some(Atom::Entity { atom }) = self.grammar.get_atom(atom_id) {
                if *atom == atom_id {
                    warnings.push(GrammarWarning::new(
                        WarningKind::InfiniteLoop,
                        atom_id,
                        format!(
                            "Atom {} is an Entity that references itself with no base case",
                            atom_id
                        ),
                    ));
                }
            }
        }
    }

    /// Detect potentially unreachable alternatives
    ///
    /// An alternative is unreachable if:
    /// 1. A string literal prefix of a later alternative is a prefix of an earlier one
    /// 2. An earlier alternative is nullable (can match empty)
    fn detect_unreachable_alternatives(&mut self, warnings: &mut Vec<GrammarWarning>) {
        for (atom_id, atom) in self.grammar.atoms.iter().enumerate() {
            if let Atom::Alternative { atoms } = atom {
                let mut nullable_seen = false;

                for (i, &child) in atoms.iter().enumerate() {
                    // If a previous alternative is nullable, this one might be unreachable
                    if nullable_seen {
                        warnings.push(GrammarWarning::new(
                            WarningKind::UnreachableAlternative,
                            atom_id,
                            format!(
                                "Alternative index {} (atom {}) may be unreachable because earlier alternative can match empty",
                                i, child
                            ),
                        ).with_related(vec![child]));
                    }

                    // Check for prefix conflicts between string literals
                    if i > 0 {
                        if let (Some(prev_lit), Some(curr_lit)) = (
                            self.get_first_literal(atoms[i - 1]),
                            self.get_first_literal(child),
                        ) {
                            if curr_lit.starts_with(&prev_lit) || prev_lit.starts_with(&curr_lit) {
                                // Only warn if they have prefix relationship and one is contained in other
                                if prev_lit == curr_lit || curr_lit.starts_with(&prev_lit) {
                                    warnings.push(GrammarWarning::new(
                                        WarningKind::UnreachableAlternative,
                                        atom_id,
                                        format!(
                                            "Alternative {} ({:?}) may shadow alternative {} ({:?})",
                                            i, curr_lit, i - 1, prev_lit
                                        ),
                                    ).with_related(vec![atoms[i - 1], child]));
                                }
                            }
                        }
                    }

                    if self.is_nullable(child) {
                        nullable_seen = true;
                    }
                }
            }
        }
    }

    /// Get the first string literal from an atom (if any)
    fn get_first_literal(&self, atom_id: usize) -> Option<String> {
        let atom = self.grammar.get_atom(atom_id)?;

        match atom {
            Atom::Str { pattern } => Some(pattern.clone()),
            Atom::Sequence { atoms } => {
                if !atoms.is_empty() {
                    self.get_first_literal(atoms[0])
                } else {
                    None
                }
            }
            Atom::Named { atom, .. }
            | Atom::Entity { atom }
            | Atom::Ignore { atom }
            | Atom::Lookahead { atom, .. } => self.get_first_literal(*atom),
            _ => None,
        }
    }

    /// Detect excessive backtracking potential
    ///
    /// This occurs with nested alternatives/repetitions that can match
    /// the same input in multiple ways, leading to exponential behavior.
    fn detect_excessive_backtracking(&mut self, warnings: &mut Vec<GrammarWarning>) {
        for (atom_id, atom) in self.grammar.atoms.iter().enumerate() {
            // Check for patterns like: (a*)* or (a|b)* where a and b overlap
            if let Atom::Repetition {
                atom: inner_atom, ..
            } = atom
            {
                // Check if inner is also a repetition
                if let Some(Atom::Repetition { .. }) = self.grammar.get_atom(*inner_atom) {
                    warnings.push(
                        GrammarWarning::new(
                            WarningKind::ExcessiveBacktracking,
                            atom_id,
                            "Nested repetitions can cause exponential backtracking".to_string(),
                        )
                        .with_related(vec![*inner_atom]),
                    );
                }

                // Check if inner is alternative with nullable branches
                if let Some(Atom::Alternative { atoms }) = self.grammar.get_atom(*inner_atom) {
                    let nullable_count = atoms.iter().filter(|&&a| self.is_nullable(a)).count();
                    if nullable_count > 1 {
                        warnings.push(GrammarWarning::new(
                            WarningKind::ExcessiveBacktracking,
                            atom_id,
                            format!(
                                "Repetition of alternative with {} nullable branches can cause backtracking",
                                nullable_count
                            ),
                        ).with_related(vec![*inner_atom]));
                    }
                }
            }

            // Check for sequences with nullable repetitions followed by matching patterns
            if let Atom::Sequence { atoms } = atom {
                for i in 0..atoms.len().saturating_sub(1) {
                    if let Some(Atom::Repetition { atom: rep_atom, .. }) =
                        self.grammar.get_atom(atoms[i])
                    {
                        // If the repetition's content can also match the next item
                        if self.can_match_same(atoms[i + 1], *rep_atom) {
                            warnings.push(
                                GrammarWarning::new(
                                    WarningKind::ExcessiveBacktracking,
                                    atom_id,
                                    "Sequence with overlapping repetition can cause backtracking"
                                        .to_string(),
                                )
                                .with_related(vec![atoms[i], atoms[i + 1]]),
                            );
                        }
                    }
                }
            }
        }
    }

    /// Check if two atoms can potentially match the same input
    fn can_match_same(&self, atom1: usize, atom2: usize) -> bool {
        let a1 = self.grammar.get_atom(atom1);
        let a2 = self.grammar.get_atom(atom2);

        match (a1, a2) {
            (Some(Atom::Str { pattern: p1 }), Some(Atom::Str { pattern: p2 })) => {
                p1.chars().next() == p2.chars().next() && !p1.is_empty() && !p2.is_empty()
            }
            (Some(Atom::Entity { atom: _ }), Some(_))
            | (Some(_), Some(Atom::Entity { atom: _ })) => {
                // Entity could match anything, be conservative
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_grammar() -> Grammar {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "hello".to_string(),
        });
        grammar.root = 0;
        grammar
    }

    #[test]
    fn test_no_warnings_for_simple_grammar() {
        let grammar = make_simple_grammar();
        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_detect_left_recursion() {
        let mut grammar = Grammar::new();
        // atom 0: Entity -> atom 0 (self-reference)
        grammar.add_atom(Atom::Entity { atom: 0 });
        grammar.root = 0;

        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::LeftRecursion));
    }

    #[test]
    fn test_detect_indirect_left_recursion() {
        let mut grammar = Grammar::new();
        // atom 0: Entity -> atom 1
        // atom 1: Entity -> atom 0
        grammar.add_atom(Atom::Entity { atom: 1 });
        grammar.add_atom(Atom::Entity { atom: 0 });
        grammar.root = 0;

        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::LeftRecursion));
    }

    #[test]
    fn test_detect_unused_atom() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "used".to_string(),
        });
        grammar.add_atom(Atom::Str {
            pattern: "unused".to_string(),
        });
        grammar.root = 0;

        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::UnusedAtom && w.atom_id == 1));
    }

    #[test]
    fn test_detect_empty_sequence() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Sequence { atoms: vec![] });
        grammar.root = 0;

        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::EmptyComposite));
    }

    #[test]
    fn test_detect_empty_alternative() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Alternative { atoms: vec![] });
        grammar.root = 0;

        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::EmptyComposite));
    }

    #[test]
    fn test_detect_useless_repetition() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        grammar.add_atom(Atom::Repetition {
            atom: 0,
            min: 0,
            max: Some(0),
        });
        grammar.root = 1;

        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::UselessRepetition));
    }

    #[test]
    fn test_detect_infinite_loop() {
        let mut grammar = Grammar::new();
        // Self-referential Entity
        grammar.add_atom(Atom::Entity { atom: 0 });
        grammar.root = 0;

        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings.iter().any(|w| w.kind == WarningKind::InfiniteLoop));
    }

    #[test]
    fn test_detect_nested_repetitions() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        grammar.add_atom(Atom::Repetition {
            atom: 0,
            min: 0,
            max: None,
        });
        grammar.add_atom(Atom::Repetition {
            atom: 1,
            min: 0,
            max: None,
        });
        grammar.root = 2;

        let warnings = GrammarAnalyzer::new(&grammar).analyze();
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::ExcessiveBacktracking));
    }

    #[test]
    fn test_nullable_detection() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "".to_string(),
        }); // Empty string is nullable
        grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        }); // Non-nullable
        grammar.add_atom(Atom::Repetition {
            atom: 1,
            min: 0,
            max: None,
        }); // Nullable (min=0)
        grammar.root = 2;

        let mut analyzer = GrammarAnalyzer::new(&grammar);
        assert!(analyzer.is_nullable(0)); // Empty string
        assert!(!analyzer.is_nullable(1)); // "a"
        assert!(analyzer.is_nullable(2)); // Repetition with min=0
    }

    #[test]
    fn test_warning_display() {
        let warning = GrammarWarning::new(WarningKind::LeftRecursion, 5, "This is left recursive")
            .with_related(vec![1, 2, 3]);

        let display = format!("{}", warning);
        assert!(display.contains("left recursion"));
        assert!(display.contains("atom 5"));
        assert!(display.contains("related atoms: [1, 2, 3]"));
    }

    #[test]
    fn test_reachable_analysis() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        }); // 0 - used
        grammar.add_atom(Atom::Str {
            pattern: "b".to_string(),
        }); // 1 - used
        grammar.add_atom(Atom::Str {
            pattern: "c".to_string(),
        }); // 2 - unused
        grammar.add_atom(Atom::Sequence { atoms: vec![0, 1] }); // 3 - used
        grammar.root = 3;

        let analyzer = GrammarAnalyzer::new(&grammar);
        let mut reachable = HashSet::new();
        analyzer.collect_reachable(3, &mut reachable);

        assert!(reachable.contains(&0));
        assert!(reachable.contains(&1));
        assert!(reachable.contains(&3));
        assert!(!reachable.contains(&2));
    }
}
