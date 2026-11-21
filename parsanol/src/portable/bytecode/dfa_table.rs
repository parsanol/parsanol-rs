//! The DFA backend table (rs#574 Phase 2, slice B).
//!
//! A `DfaTable` is the deterministic recognizer for one provable rule
//! (see `Compiler::rule_lr_provability`): dense `states × 256`
//! transitions, `u32::MAX` = no transition (stuck), plus the accept
//! set. The provability proof (loop-exit disjointness, pairwise-disjoint
//! choice FIRST sets, no recursion) means the epsilon-NFA built from
//! the atom tree determinizes single-path — no backtracking, so the
//! greedy longest walk is exact for every usage site.
//!
//! The routed repetition-loop tables ride the same shape; their
//! iteration-accept states live in the program's parallel registry
//! (`Program::get_dfa_iteration_accepts`) so this struct stays at its
//! published field set.

/// A dense byte-transition table for one lowered rule.
#[derive(Debug, Clone)]
pub struct DfaTable {
    /// `state * 256 + byte` → next state (or `u32::MAX` when stuck).
    pub transitions: Vec<u32>,
    /// State ids that end a valid rule match.
    pub accepts: Vec<bool>,
    /// The start state id (always 0 after the builder).
    pub start: u32,
    /// Number of states (the transition table's row count).
    pub state_count: u32,
}

impl DfaTable {
    /// The next state for `(state, byte)`, or `None` when stuck.
    #[inline]
    pub fn step(&self, state: u32, byte: u8) -> Option<u32> {
        let next = self.transitions[(state as usize) * 256 + byte as usize];
        if next == u32::MAX {
            None
        } else {
            Some(next)
        }
    }

    /// Whether the state ends a valid match.
    #[inline]
    pub fn is_accept(&self, state: u32) -> bool {
        self.accepts[state as usize]
    }
}

/// The routed repetition loop's executor metadata: the iteration-accept
/// states and the routed minimum. Lives in the program's parallel
/// registry so `DfaTable` and `Instruction::DfaRun` keep their
/// published shapes.
#[derive(Debug, Clone)]
pub struct DfaLoopMeta {
    /// States that complete one repetition iteration: the envelope
    /// executor pushes the iteration's span crossing one of these.
    pub iteration_accepts: Vec<bool>,
    /// The lower bound of the routed repetition.
    pub min: u32,
}
