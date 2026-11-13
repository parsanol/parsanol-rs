//! The parser's derived analyses (rs#486): five pure functions of the
//! grammar's atom list, computed once per DECODED ENTRY and shared by
//! every parse of it.
//!
//! They deliberately do NOT live on [`Grammar`]: the struct is
//! exhaustively constructible through the public API, and any new
//! field — public or private — breaks downstream struct literals
//! (cargo-semver-checks constructible_struct_adds_field). The cache
//! lives with whoever owns a long-lived grammar: the PARG artifact's
//! decoded-entry cache (all-private struct, additive), handed to the
//! parser through `PortableParser::with_analysis`. Plain constructors
//! compute per call (the hand-built-grammar path, unchanged behavior).

use super::char_class::CharacterPattern;
use super::grammar::{Atom, Grammar};
use std::collections::HashMap;
use std::sync::Arc;

/// A pure-function-of-atoms analysis set the parser reads at
/// construction. All five are deterministic over `atoms`.
#[derive(Debug, Clone, Default)]
pub struct ParserAnalysis {
    /// Memoization is skipped for Dynamic/StateMatch/StateSwitch atoms
    /// and every ancestor that can reach one.
    pub dynamic_dependent: Arc<Vec<bool>>,
    /// A pure terminal (literal, or regex on the direct class path)
    /// Strict mode evaluates without the memo.
    pub terminal_direct: Arc<Vec<bool>>,
    /// StateSet writers and ancestors: dyn-cache replay re-derives the
    /// value but skips the slot write, so these bypass the dyn cache.
    pub state_write_dependent: Arc<Vec<bool>>,
    /// Capture writers and ancestors whose writes escape their boundary;
    /// scope/lookahead isolate the propagation.
    pub capture_write_dependent: Arc<Vec<bool>>,
    /// Rule-name → Named-atom index for StateSwitch dispatch.
    pub named_atoms: Arc<HashMap<String, usize>>,
}

fn dependence_closure_until(
    grammar: &Grammar,
    is_seed: impl Fn(&Atom) -> bool,
    is_barrier: impl Fn(&Atom) -> bool,
) -> Vec<bool> {
    let has_dynamic = grammar.atoms.iter().any(|a| is_seed(a));
    if !has_dynamic {
        return Vec::new();
    }

    let n = grammar.atoms.len();
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, atom) in grammar.atoms.iter().enumerate() {
        if is_barrier(atom) {
            continue;
        }
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
        .filter_map(|(i, a)| is_seed(a).then_some(i))
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

impl ParserAnalysis {
    /// Compute all five analyses over a grammar. O(atoms); the result
    /// is deterministic and shareable.
    pub fn compute(grammar: &Grammar) -> Self {
        let dynamic_dependent = dependence_closure_until(
            grammar,
            |a| {
                matches!(
                    a,
                    Atom::Dynamic { .. } | Atom::StateMatch { .. } | Atom::StateSwitch { .. }
                )
            },
            |_| false,
        );
        // Ancestors of StateSet writes (parsanol-ruby#185): a dyn-cache
        // replay re-derives the VALUE but skips the slot write, so a later
        // reader sees a stale slot. These atoms bypass the dyn cache.
        let state_write_dependent =
            dependence_closure_until(grammar, |a| matches!(a, Atom::StateSet { .. }), |_| false);
        // Capture writes are observable after their atom succeeds, but memo
        // entries contain only the parsed value and end position. Replaying a
        // capture-producing subtree after rollback would skip its writes. Scope
        // and lookahead isolate those writes, so they stop this propagation and
        // remain eligible for memoization themselves.
        let capture_write_dependent = dependence_closure_until(
            grammar,
            |a| matches!(a, Atom::Capture { .. }),
            |a| matches!(a, Atom::Scope { .. } | Atom::Lookahead { .. }),
        );
        let terminal_direct: Vec<bool> = grammar
            .atoms
            .iter()
            .map(|a| match a {
                Atom::Str { .. } => true,
                Atom::Re { pattern } => {
                    CharacterPattern::from_pattern(pattern).is_some()
                        || crate::portable::regex_cache::get_or_decode_class(pattern).is_some()
                }
                _ => false,
            })
            .collect();
        // Rule-name → Named-atom index for StateSwitch dispatch: the wire
        // inlines rule bodies, so a rule name resolves to the Named atom
        // that carries it (first occurrence wins — inlined copies are
        // equivalent).
        let mut named_atoms = HashMap::new();
        for (i, atom) in grammar.atoms.iter().enumerate() {
            if let Atom::Named { name, .. } = atom {
                named_atoms.entry(name.clone()).or_insert(i);
            }
        }
        Self {
            dynamic_dependent: Arc::new(dynamic_dependent),
            terminal_direct: Arc::new(terminal_direct),
            state_write_dependent: Arc::new(state_write_dependent),
            capture_write_dependent: Arc::new(capture_write_dependent),
            named_atoms: Arc::new(named_atoms),
        }
    }
}
