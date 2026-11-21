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

/// One sequence element's value descriptor (tranche 4, corrected in
/// tranche 5): the characterization matrix (#631) showed the classic
/// BuildSeq fold NEVER merges — every element lands as its own span.
/// The bare variants record provenance only (the nested-fragments
/// slice reads the producing path); the executor keys on `Hash`
/// alone.
#[derive(Debug, Clone)]
pub enum ElementDesc {
    /// A bare span from an inline Str/Re instruction.
    InlineBare,
    /// A bare span delivered through a rule-ref call.
    RefBare,
    /// A hash wrap `{name: span}` (a Named-wrapped terminal or a
    /// Named rule ref — the v2 named-tree law).
    Hash(String),
}

/// The routed sequence loop's element plan TREE (tranche 6): the
/// nested extension of [`ElementDesc`], on its own type and its own
/// program registry so the tranche-4 shapes stay frozen (the
/// additive-registry law). The executor prefers the plan tree; the
/// `DfaLoopElements` names stay the flat per-leaf fallback.
#[derive(Debug, Clone)]
pub enum ElementPlan {
    /// A bare span from an inline Str/Re instruction.
    Bare,
    /// A bare span delivered through a rule-ref call.
    RefBare,
    /// A hash wrap `{name: span}` around one bare leaf.
    Hash(String),
    /// A bare nested sequence: ONE `:sequence` node whose values are
    /// the children's — the classic emits one node per Sequence atom,
    /// even a builder-fused one-element inner.
    Nested(Vec<ElementPlan>),
    /// A hash wrap `{name -> :sequence node}` around a nested
    /// sequence's values (the v2 named-tree law composes).
    HashNested(String, Vec<ElementPlan>),
}

/// The routed sequence loop's per-element synthesis plan (tranche 4,
/// per-leaf cut indices in tranche 6): which DFA states close a LEAF
/// span of the iteration (nested elements flatten to their leaves —
/// the cut positions) and the flat per-leaf descriptors. The nested
/// plan tree rides the program's `dfa_loop_element_plans` registry and
/// supersedes `names` in the executor when present. Parallel to
/// `dfa_tables`.
#[derive(Debug, Clone)]
pub struct DfaLoopElements {
    /// Per-DFA-state: the LEAF index whose accept the state closes, or
    /// `None`. The LAST leaf's accept is also the iteration boundary.
    pub accepts_per_state: Vec<Option<u8>>,
    /// Per LEAF (the flattened order the DFA chain records): the flat
    /// fallback descriptor. For flat bodies this is one entry per
    /// element; nested bodies' tree detail lives in the plan registry.
    pub names: Vec<ElementDesc>,
}
