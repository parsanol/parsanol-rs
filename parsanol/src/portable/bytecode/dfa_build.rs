//! The DFA table builder (rs#574 Phase 2, slice B).
//!
//! Builds a [`DfaTable`] for a rule the provability prover accepted.
//! The construction is the classic Thompson NFA over the atom tree,
//! then subset determinization — the provability proof (pairwise
//! disjoint choice FIRST sets, loop-exit disjointness, no recursion)
//! means the determinized table is single-path, so the greedy longest
//! walk is exact.
//!
//! Stage 1 scope: capture-free rules (pure spans — the string/ws/
//! number interiors). Named markers inside provable bodies ride a
//! later slice (accept-state → value-synthesis metadata). The builder
//! is the final authority: anything it cannot express returns `None`
//! and the routing keeps the existing lowering.

use super::dfa_table::DfaTable;
use crate::portable::char_class::CharRanges;
use crate::portable::grammar::{Atom, Grammar, RepetitionTag};

/// One Thompson NFA edge: a byte-class bitmap or epsilon.
#[derive(Debug, Clone)]
enum Edge {
    Epsilon,
    Bytes(Box<[bool; 256]>),
}

#[derive(Default)]
struct Nfa {
    byte_edges: Vec<(u32, [bool; 256], u32)>,
    eps: Vec<(u32, u32)>,
    nodes: u32,
}

impl Nfa {
    fn node(&mut self) -> u32 {
        let id = self.nodes;
        self.nodes += 1;
        id
    }

    fn edge(&mut self, from: u32, edge: Edge, to: u32) {
        match edge {
            Edge::Epsilon => self.eps.push((from, to)),
            Edge::Bytes(bits) => self.byte_edges.push((from, *bits, to)),
        }
    }
}

/// The byte bitmap for one literal string (the byte-sequence chain
/// uses single-byte edges; this is the single-edge form for classes).
fn bits_of(bytes: &[u8]) -> Box<[bool; 256]> {
    let mut bits = [false; 256];
    for &b in bytes {
        bits[b as usize] = true;
    }
    Box::new(bits)
}

/// The single-byte ASCII class for an `Re` atom, or `None` when the
/// pattern spans codepoints the byte-level builder refuses.
fn ascii_class_bits(pattern: &str) -> Option<Box<[bool; 256]>> {
    let ranges = CharRanges::from_pattern(pattern)?;
    let mut bits = [false; 256];
    for (lo, hi) in ranges.codepoint_ranges() {
        // Stage 1: ASCII single-byte codepoints only — a multi-byte
        // codepoint edge is a byte-sequence chain, and a mixed class's
        // UTF-8 prefixes need their own construction (a later slice).
        if lo > 0x7F || hi > 0x7F {
            return None;
        }
        for cp in lo..=hi.min(lo + 0xFFFF) {
            bits[cp as usize] = true;
        }
        if hi > lo + 0xFFFF {
            break;
        }
    }
    Some(Box::new(bits))
}

/// Build the DFA table for `rule_idx`, or `None` when the atom tree
/// carries something the byte-level builder refuses. `in_progress`
/// guards recursion (the provability verdict already refused it — this
/// is the builder's own belt).
pub fn build_rule_dfa(grammar: &Grammar, rule_idx: usize) -> Option<DfaTable> {
    let mut nfa = Nfa::default();
    let (start, accept) = thompson(grammar, rule_idx, &mut nfa, &mut Vec::new())?;
    Some(determinize(&nfa, start, accept))
}

/// The Thompson fragment for one atom: `(start, accept)` node ids.
fn thompson(
    grammar: &Grammar,
    idx: usize,
    nfa: &mut Nfa,
    in_progress: &mut Vec<usize>,
) -> Option<(u32, u32)> {
    if in_progress.contains(&idx) {
        return None;
    }
    let atom = grammar.get_atom(idx)?;
    in_progress.push(idx);
    let fragment = match atom {
        Atom::Str { pattern } => {
            let bytes = pattern.as_bytes();
            if bytes.is_empty() {
                let n = nfa.node();
                Some((n, n))
            } else {
                let mut nodes = Vec::with_capacity(bytes.len() + 1);
                for _ in 0..=bytes.len() {
                    nodes.push(nfa.node());
                }
                for (i, &b) in bytes.iter().enumerate() {
                    nfa.edge(nodes[i], Edge::Bytes(bits_of(&[b])), nodes[i + 1]);
                }
                Some((*nodes.first()?, *nodes.last()?))
            }
        }
        Atom::Re { pattern } => {
            let bits = ascii_class_bits(pattern)?;
            let (s, a) = (nfa.node(), nfa.node());
            nfa.edge(s, Edge::Bytes(bits), a);
            Some((s, a))
        }
        Atom::Sequence { atoms } => {
            // Chain the fragments with epsilon joins.
            let mut acc: Option<(u32, u32)> = None;
            for &child in atoms {
                let (cs, ca) = thompson(grammar, child, nfa, in_progress)?;
                acc = Some(match acc {
                    None => (cs, ca),
                    Some((ps, pa)) => {
                        nfa.edge(pa, Edge::Epsilon, cs);
                        (ps, ca)
                    }
                });
            }
            acc
        }
        Atom::Alternative { atoms } => {
            let (s, a) = (nfa.node(), nfa.node());
            for &branch in atoms {
                let (bs, body_accept) = thompson(grammar, branch, nfa, in_progress)?;
                nfa.edge(s, Edge::Epsilon, bs);
                nfa.edge(body_accept, Edge::Epsilon, a);
            }
            Some((s, a))
        }
        Atom::Repetition {
            atom: body,
            min,
            max,
            tag: RepetitionTag::Repetition | RepetitionTag::Maybe,
        } => {
            // Mandatory iterations chained; the optional remainder as
            // epsilon-skipped copies; the unlimited tail as a loop.
            // Nullable bodies were refused by the provability verdict;
            // the builder refuses them again as its own belt.
            let body_states = fragment_states(grammar, *body, 0, &mut Vec::new())?;
            if max.is_some_and(|m| (m.saturating_sub(*min)) as u64 * body_states as u64 > 4096) {
                return None; // the bounded-unroll sanity cap
            }
            let (entry, exit) = (nfa.node(), nfa.node());
            let mut cursor = entry;
            for _ in 0..*min {
                let (bs, body_accept) = thompson(grammar, *body, nfa, in_progress)?;
                nfa.edge(cursor, Edge::Epsilon, bs);
                cursor = body_accept;
            }
            match max {
                None => {
                    let (bs, body_accept) = thompson(grammar, *body, nfa, in_progress)?;
                    if *min == 0 {
                        // The zero-iteration path: without it the start
                        // closure never reaches the exit.
                        nfa.edge(cursor, Edge::Epsilon, exit);
                    }
                    nfa.edge(cursor, Edge::Epsilon, bs);
                    nfa.edge(body_accept, Edge::Epsilon, bs); // the loop back
                    nfa.edge(body_accept, Edge::Epsilon, exit);
                }
                Some(m) => {
                    for _ in *min..*m {
                        // epsilon-optional copy
                        let skip = nfa.node();
                        nfa.edge(cursor, Edge::Epsilon, skip);
                        let (bs, body_accept) = thompson(grammar, *body, nfa, in_progress)?;
                        nfa.edge(cursor, Edge::Epsilon, bs);
                        nfa.edge(body_accept, Edge::Epsilon, exit);
                        cursor = skip;
                    }
                    nfa.edge(cursor, Edge::Epsilon, exit);
                }
            }
            Some((entry, exit))
        }
        Atom::Named { atom, .. }
        | Atom::Capture { atom, .. }
        | Atom::Scope { atom }
        | Atom::Ignore { atom }
        | Atom::Trivia { atom }
        | Atom::TriviaCapture { atom, .. } => thompson(grammar, *atom, nfa, in_progress),
        Atom::Entity { atom } => thompson(grammar, *atom, nfa, in_progress),
        Atom::Cut => {
            let n = nfa.node();
            Some((n, n))
        }
        // Predicates, constants, host-computed and stateful atoms: the
        // provability verdict refuses these; the builder agrees.
        Atom::Lookahead { .. }
        | Atom::Lookbehind { .. }
        | Atom::Constant { .. }
        | Atom::Dynamic { .. }
        | Atom::Custom { .. }
        | Atom::CustomRef { .. }
        | Atom::StateSet { .. }
        | Atom::StateMatch { .. }
        | Atom::StateSwitch { .. } => None,
    };
    in_progress.pop();
    fragment
}

/// The NFA node count one atom's fragment would occupy (the bounded
/// unroll's sanity check, without building it).
fn fragment_states(
    grammar: &Grammar,
    idx: usize,
    depth: usize,
    seen: &mut Vec<usize>,
) -> Option<u32> {
    if depth > 64 || seen.contains(&idx) {
        return None;
    }
    seen.push(idx);
    let atom = grammar.get_atom(idx)?;
    let n = match atom {
        Atom::Str { pattern } => pattern.len().max(1) as u32,
        Atom::Re { pattern } => {
            let _ = ascii_class_bits(pattern)?;
            1
        }
        Atom::Sequence { atoms } | Atom::Alternative { atoms } => {
            let mut total = 0;
            for &child in atoms {
                total += fragment_states(grammar, child, depth + 1, seen)?;
            }
            total + 1
        }
        Atom::Repetition { atom, max, .. } => {
            let body = fragment_states(grammar, *atom, depth + 1, seen)?;
            match max {
                None => body + 2,
                Some(m) => body.saturating_mul((*m).max(1) as u32) + 2,
            }
        }
        Atom::Named { atom, .. }
        | Atom::Capture { atom, .. }
        | Atom::Scope { atom }
        | Atom::Ignore { atom }
        | Atom::Trivia { atom }
        | Atom::TriviaCapture { atom, .. } => fragment_states(grammar, *atom, depth + 1, seen)?,
        Atom::Entity { atom } => fragment_states(grammar, *atom, depth + 1, seen)?,
        Atom::Cut => 1,
        _ => return None,
    };
    seen.pop();
    Some(n)
}

/// Subset determinization over epsilon closures into the dense table.
fn determinize(nfa: &Nfa, start: u32, accept: u32) -> DfaTable {
    determinize_marked(
        nfa,
        start,
        accept,
        &std::collections::HashSet::new(),
        &std::collections::HashMap::new(),
        None,
    )
    .0
}

/// The deterministic table for the UNBOUNDED LOOP of one body atom:
/// star(body) with the body's fragment-accept nodes marked — a DFA
/// state containing a marked node completes one iteration, which the
/// envelope executor turns into the iteration's span. Any `min` is
/// enforced by the executor against the crossing count.
pub fn build_loop_dfa(
    grammar: &Grammar,
    body_idx: usize,
) -> Option<(DfaTable, Vec<bool>, Vec<Option<u8>>)> {
    // The body must consume at least one byte: a nullable body makes
    // the loop's iteration boundaries degenerate.
    if !consumes_min_one(grammar, body_idx, 0, &mut Vec::new()) {
        return None;
    }
    let mut nfa = Nfa::default();
    let entry = nfa.node();
    let exit = nfa.node();

    // A flat Sequence body: build the chain LEAF BY LEAF so each
    // leaf's accept node is known — the cut positions the executor
    // assembles the element tree's values from (tranche 4; nested
    // elements flatten to their leaves in tranche 6, whose plan tree
    // rides the compiler's descriptor). The leaves cross in chain
    // order, so a DFA subset contains at most one leaf accept (the
    // disjointness proof keeps the chain linear).
    let mut element_accepts_of_node: std::collections::HashMap<u32, u8> =
        std::collections::HashMap::new();
    // Transparent wrappers (Named/Entity) unwrap before the chain
    // check: a Named-wrapped sequence body carries the element plan
    // too (the wrapper's hash rides the executor's named_idx).
    let mut body_idx = body_idx;
    while let Some(Atom::Named { atom, .. }) | Some(Atom::Entity { atom }) =
        grammar.get_atom(body_idx)
    {
        body_idx = *atom;
    }
    // The flattened leaf atom indices of one element (tranche 6):
    // unwrapping Named/Entity around bare terminals; nested Sequence
    // elements contribute their leaves depth-first. The wrappers'
    // values (hash wraps, nested nodes) are the PLAN's job — the NFA
    // only needs the byte chain, and chaining the leaves equals the
    // whole-subtree Thompson fragment (Named/Entity descend
    // transparently). The unwrap set mirrors the compiler's element
    // plan EXACTLY (Named/Entity transparent, everything else a leaf
    // or a refusal) so the per-leaf accept indices and the plan tree
    // never desync.
    fn leaf_chain(grammar: &Grammar, idx: usize, leaves: &mut Vec<usize>, depth: u8) -> Option<()> {
        if depth > 64 {
            return None;
        }
        match grammar.get_atom(idx)? {
            Atom::Named { atom, .. } | Atom::Entity { atom } => {
                leaf_chain(grammar, *atom, leaves, depth + 1)
            }
            Atom::Sequence { atoms } if !atoms.is_empty() => {
                for &child in atoms {
                    leaf_chain(grammar, child, leaves, depth + 1)?;
                }
                Some(())
            }
            Atom::Str { pattern } if !pattern.is_empty() => {
                leaves.push(idx);
                Some(())
            }
            Atom::Re { pattern } => {
                CharRanges::from_pattern(pattern)?;
                leaves.push(idx);
                Some(())
            }
            _ => None,
        }
    }
    let (bs, body_accept) = match grammar.get_atom(body_idx) {
        Some(Atom::Sequence { atoms }) if !atoms.is_empty() => {
            let mut cursor = entry;
            let mut first_start = entry;
            let mut last_accept = entry;
            let mut leaf_i = 0u8;
            let mut first_element = true;
            for &child in atoms.iter() {
                let mut leaves = Vec::with_capacity(2);
                leaf_chain(grammar, child, &mut leaves, 0)?;
                if leaves.is_empty() || leaves.len() > u8::MAX as usize {
                    return None;
                }
                let (mut cs, mut ca) = (entry, entry);
                for (j, &leaf) in leaves.iter().enumerate() {
                    let (ls, la) = thompson(grammar, leaf, &mut nfa, &mut Vec::new())?;
                    if j == 0 {
                        cs = ls;
                    } else {
                        nfa.edge(ca, Edge::Epsilon, ls);
                    }
                    ca = la;
                    element_accepts_of_node.insert(la, leaf_i);
                    leaf_i += 1;
                }
                nfa.edge(cursor, Edge::Epsilon, cs);
                cursor = ca;
                if first_element {
                    first_start = cs;
                    first_element = false;
                }
                last_accept = ca;
            }
            (first_start, last_accept)
        }
        _ => thompson(grammar, body_idx, &mut nfa, &mut Vec::new())?,
    };
    nfa.edge(entry, Edge::Epsilon, bs);
    nfa.edge(body_accept, Edge::Epsilon, bs); // the loop back
    nfa.edge(body_accept, Edge::Epsilon, exit);
    let mut marked = std::collections::HashSet::new();
    marked.insert(body_accept);
    let (table, iteration_accepts, element_accepts, _inner) =
        determinize_marked(&nfa, entry, exit, &marked, &element_accepts_of_node, None);
    Some((table, iteration_accepts, element_accepts))
}

/// The envelope-aware loop table (tranche 7): `build_loop_dfa`'s
/// superset. Bodies whose elements are THEMSELVES loops chain the
/// inner loop's copies into the byte chain and mark each copy accept
/// as an INNER boundary — `inner_accepts` maps DFA states to the
/// element whose inner iteration they close (the envelope executor
/// appends a per-inner-iteration cut). Flat/nested bodies produce the
/// identical table with an all-`None` `inner_accepts`.
pub struct LoopDfa {
    /// The deterministic table for the loop body.
    pub table: DfaTable,
    /// Per state: whether it completes one OUTER iteration.
    pub iteration_accepts: Vec<bool>,
    /// Per state: the leaf slot whose boundary the state closes.
    pub element_accepts: Vec<Option<u8>>,
    /// Per state: the envelope ordinal whose inner iteration the
    /// state closes.
    pub inner_accepts: Vec<Option<u8>>,
}

/// Build the envelope-aware loop table for `body_idx`, or `None`
/// when the atom tree carries something the byte-level builder
/// refuses (predicates, recursion, non-chainable bodies, unbounded
/// unrolls). Identical to [`build_loop_dfa`] for bodies without
/// envelope elements.
pub fn build_loop_dfa_envelopes(grammar: &Grammar, body_idx: usize) -> Option<LoopDfa> {
    // The body must consume at least one byte: a nullable body makes
    // the loop's iteration boundaries degenerate.
    if !consumes_min_one(grammar, body_idx, 0, &mut Vec::new()) {
        return None;
    }
    let mut nfa = Nfa::default();
    let entry = nfa.node();
    let exit = nfa.node();

    let mut element_accepts_of_node: std::collections::HashMap<u32, u8> =
        std::collections::HashMap::new();
    let mut inner_accepts_of_node: std::collections::HashMap<u32, u8> =
        std::collections::HashMap::new();
    let mut body_idx = body_idx;
    while let Some(Atom::Named { atom, .. }) | Some(Atom::Entity { atom }) =
        grammar.get_atom(body_idx)
    {
        body_idx = *atom;
    }
    // The flattened leaf chain of one NON-envelope element (tranche 6
    // semantics, unchanged).
    fn leaf_chain(grammar: &Grammar, idx: usize, leaves: &mut Vec<usize>, depth: u8) -> Option<()> {
        if depth > 64 {
            return None;
        }
        match grammar.get_atom(idx)? {
            Atom::Named { atom, .. } | Atom::Entity { atom } => {
                leaf_chain(grammar, *atom, leaves, depth + 1)
            }
            Atom::Sequence { atoms } if !atoms.is_empty() => {
                for &child in atoms {
                    leaf_chain(grammar, child, leaves, depth + 1)?;
                }
                Some(())
            }
            Atom::Str { pattern } if !pattern.is_empty() => {
                leaves.push(idx);
                Some(())
            }
            Atom::Re { pattern } => {
                CharRanges::from_pattern(pattern)?;
                leaves.push(idx);
                Some(())
            }
            _ => None,
        }
    }
    // Whether an atom is an ENVELOPE element (tranche 7): a repetition
    // (Named/Entity wrappers unwrap) whose body leaf-chains. Bounded
    // copies respect the unroll sanity cap. Envelopes nest ONE level —
    // an envelope body containing another repetition refuses.
    fn envelope_of(grammar: &Grammar, idx: usize) -> Option<(usize, usize, Option<usize>)> {
        let (idx, min, max) = match grammar.get_atom(idx)? {
            Atom::Repetition {
                atom: body,
                min,
                max,
                tag: RepetitionTag::Repetition | RepetitionTag::Maybe,
                ..
            } => (*body, *min, *max),
            Atom::Named { atom, .. } | Atom::Entity { atom } => {
                let (b, mi, ma) = envelope_of(grammar, *atom)?;
                (b, mi, ma)
            }
            _ => return None,
        };
        // One envelope level: the body must leaf-chain without hitting
        // another repetition.
        fn no_inner_loop(grammar: &Grammar, idx: usize, depth: u8) -> Option<()> {
            if depth > 64 {
                return None;
            }
            match grammar.get_atom(idx)? {
                Atom::Named { atom, .. } | Atom::Entity { atom } => {
                    no_inner_loop(grammar, *atom, depth + 1)
                }
                Atom::Sequence { atoms } if !atoms.is_empty() => {
                    for &child in atoms {
                        no_inner_loop(grammar, child, depth + 1)?;
                    }
                    Some(())
                }
                Atom::Str { pattern } if !pattern.is_empty() => Some(()),
                Atom::Re { pattern } => {
                    CharRanges::from_pattern(pattern)?;
                    Some(())
                }
                _ => None,
            }
        }
        no_inner_loop(grammar, idx, 0)?;
        if max.is_some_and(|m| (m.saturating_sub(min)) as u64 * 8 > 4096) {
            return None;
        }
        Some((idx, min, max))
    }
    // Whether an atom is a CHOICE of bare span leaves (tranche 8): an
    // Alternative (Named/Entity wrappers unwrap) whose arms each resolve
    // to one non-empty Str or class Re leaf. The arm atom ids return in
    // order; anything else — an arm that is a sequence, a nested
    // alternative, an envelope, a named wrap around a non-leaf — refuses
    // (the shape falls to the tranche-9 builder or the classic path).
    fn choice_span_arms(grammar: &Grammar, idx: usize, depth: u8) -> Option<Vec<usize>> {
        if depth > 64 {
            return None;
        }
        let bare_leaf = |i: usize| -> Option<usize> {
            let mut cur = i;
            for _ in 0..16 {
                match grammar.get_atom(cur)? {
                    Atom::Entity { atom } | Atom::Named { atom, .. } => cur = *atom,
                    other => {
                        return match other {
                            Atom::Str { pattern } if !pattern.is_empty() => Some(cur),
                            Atom::Re { pattern } => {
                                CharRanges::from_pattern(pattern)?;
                                Some(cur)
                            }
                            _ => None,
                        };
                    }
                }
            }
            None
        };
        let mut cur = idx;
        for _ in 0..16 {
            match grammar.get_atom(cur)? {
                Atom::Entity { atom } | Atom::Named { atom, .. } => cur = *atom,
                Atom::Alternative { atoms } if !atoms.is_empty() => {
                    let mut arms = Vec::with_capacity(atoms.len());
                    for &a in atoms {
                        arms.push(bare_leaf(a)?);
                    }
                    return Some(arms);
                }
                _ => return None,
            }
        }
        None
    }
    // Tranche 9's recursive fragment builder (the per-arm CHOICE):
    // walks the atom tree building BOTH the NFA fragment and the plan
    // in lockstep - the fragment starts at `fork` (the caller wired
    // it), every leaf's accept takes its sequential slot and
    // epsilon-joins into the returned join node. v1 scope: leaves,
    // flat sequences, Named wraps around leaves/sequences, and nested
    // choices; envelope elements inside tranche-9 shapes refuse (the
    // classic path keeps those bodies).
    #[allow(clippy::too_many_arguments)]
    fn build_plan9(
        grammar: &Grammar,
        atom_idx: usize,
        nfa: &mut Nfa,
        fork: u32,
        element_accepts_of_node: &mut std::collections::HashMap<u32, u8>,
        first_slot: u8,
        depth: u8,
    ) -> Option<(u32, super::dfa_table::ElementPlanTree9, u8)> {
        if depth > 64 || first_slot > u8::MAX - 16 {
            return None;
        }
        // Unwrap Named/Entity, remembering the outermost name (the
        // plan's hash wrap; the NFA is name-blind).
        let mut cur = atom_idx;
        let mut name: Option<String> = None;
        let plan_atom: &Atom = loop {
            match grammar.get_atom(cur)? {
                Atom::Entity { atom } => cur = *atom,
                Atom::Named { atom, name: n } => {
                    if name.is_none() {
                        name = Some(n.clone());
                    }
                    cur = *atom;
                }
                other => break other,
            }
        };
        let join = nfa.node();
        match plan_atom {
            Atom::Str { .. } | Atom::Re { .. } => {
                let (ls, la) = thompson(grammar, cur, nfa, &mut Vec::new())?;
                nfa.edge(fork, Edge::Epsilon, ls);
                element_accepts_of_node.insert(la, first_slot);
                nfa.edge(la, Edge::Epsilon, join);
                let plan = match name {
                    Some(n) => super::dfa_table::ElementPlanTree9::Hash(n),
                    None => super::dfa_table::ElementPlanTree9::Bare,
                };
                Some((join, plan, 1))
            }
            Atom::Sequence { atoms } if !atoms.is_empty() => {
                let mut child_plans = Vec::with_capacity(atoms.len());
                let mut slot = first_slot;
                let mut from = fork;
                for &child in atoms {
                    let (child_join, plan, count) = build_plan9(
                        grammar,
                        child,
                        nfa,
                        from,
                        element_accepts_of_node,
                        slot,
                        depth + 1,
                    )?;
                    child_plans.push(plan);
                    slot = slot.saturating_add(count);
                    from = child_join;
                }
                let plan = match name {
                    Some(n) => super::dfa_table::ElementPlanTree9::HashNested(n, child_plans),
                    None => super::dfa_table::ElementPlanTree9::Nested(child_plans),
                };
                let count = slot.saturating_sub(first_slot);
                Some((from, plan, count))
            }
            Atom::Alternative { atoms } if !atoms.is_empty() => {
                // A named wrap around a choice refuses (v1: unnamed
                // arms only; the classic path keeps those).
                if name.is_some() {
                    return None;
                }
                let mut arm_plans = Vec::with_capacity(atoms.len());
                let mut total = 0u8;
                for &arm in atoms {
                    let (arm_join, plan, count) = build_plan9(
                        grammar,
                        arm,
                        nfa,
                        fork,
                        element_accepts_of_node,
                        first_slot.saturating_add(total),
                        depth + 1,
                    )?;
                    nfa.edge(arm_join, Edge::Epsilon, join);
                    arm_plans.push(super::dfa_table::ElementPlanChoiceArm9 {
                        plan: Box::new(plan),
                        leaf_count: count,
                    });
                    total = total.saturating_add(count);
                }
                Some((
                    join,
                    super::dfa_table::ElementPlanTree9::Choice(arm_plans),
                    total,
                ))
            }
            _ => None,
        }
    }
    let (bs, body_accept) = match grammar.get_atom(body_idx) {
        Some(Atom::Sequence { atoms }) if !atoms.is_empty() => {
            let mut cursor = entry;
            let mut first_start = entry;
            let mut last_accept = entry;
            let mut leaf_i = 0u8;
            let mut env_i = 0u8;
            let mut first_element = true;
            for &child in atoms.iter() {
                if let Some((inner_body, inner_min, inner_max)) = envelope_of(grammar, child) {
                    // The envelope element (tranche 7): chain the inner
                    // loop's copies into the byte chain; every copy
                    // accept is an INNER boundary (the envelope
                    // executor's per-inner-iteration cut); ONE fixed
                    // leaf slot carries the element's outer boundary.
                    let mut inner_leaves = Vec::with_capacity(2);
                    leaf_chain(grammar, inner_body, &mut inner_leaves, 0)?;
                    if inner_leaves.is_empty() || inner_leaves.len() > u8::MAX as usize {
                        return None;
                    }
                    let mut chain_inner_copy =
                        |nfa: &mut Nfa, from: u32, element: u8| -> Option<u32> {
                            let mut from = from;
                            let mut copy_accept = from;
                            for (j, &leaf) in inner_leaves.iter().enumerate() {
                                let (ls, la) = thompson(grammar, leaf, nfa, &mut Vec::new())?;
                                nfa.edge(from, Edge::Epsilon, ls);
                                copy_accept = la;
                                if j == inner_leaves.len() - 1 {
                                    inner_accepts_of_node.insert(la, element);
                                }
                                from = la;
                            }
                            Some(copy_accept)
                        };
                    let mut pos = cursor;
                    for _ in 0..inner_min {
                        pos = chain_inner_copy(&mut nfa, pos, env_i)?;
                    }
                    match inner_max {
                        None => {
                            // The unbounded tail: one copy, the loop
                            // back to the tail's start, and a fresh
                            // connector node. The zero-path edge from
                            // the iteration start is REQUIRED (an
                            // empty envelope must reach the next
                            // element). It merges the connector into
                            // every iteration-boundary state —
                            // harmless NOW because the envelope
                            // element carries NO markers at all: its
                            // boundary info lives entirely in the
                            // inner lane (the last copy accept IS the
                            // boundary), keeping every state
                            // single-marked in both lanes.
                            let tail_start = pos;
                            let accept = chain_inner_copy(&mut nfa, pos, env_i)?;
                            nfa.edge(accept, Edge::Epsilon, tail_start);
                            let boundary = nfa.node();
                            nfa.edge(accept, Edge::Epsilon, boundary);
                            if inner_min == 0 {
                                nfa.edge(cursor, Edge::Epsilon, boundary);
                            } else {
                                // The mandatory copies' EXIT: after
                                // inner_min iterations an unbounded
                                // loop may stop — the last mandatory
                                // copy's accept reaches the boundary
                                // too. (Tranche-8 catch: the arithmetic
                                // tail's `1*digit` number element
                                // demanded a SECOND inner iteration
                                // before any outer iteration could
                                // close — the min=0 fixtures never
                                // walked this path.)
                                nfa.edge(tail_start, Edge::Epsilon, boundary);
                            }
                            pos = boundary;
                        }
                        Some(m) => {
                            // Optional copies (max > min) each get a
                            // skip edge; boundaries are pure
                            // connectors (no markers).
                            for _ in inner_min..m {
                                let boundary = nfa.node();
                                nfa.edge(pos, Edge::Epsilon, boundary);
                                let accept = chain_inner_copy(&mut nfa, pos, env_i)?;
                                nfa.edge(accept, Edge::Epsilon, boundary);
                                pos = boundary;
                            }
                            if inner_min == 0 {
                                // min == 0, bounded: the zero-copy
                                // path IS `pos` (the connector) — no
                                // marker needed; the empty envelope
                                // rides the assembly's prev walk.
                            }
                        }
                    }
                    env_i += 1;
                    if env_i == 0 {
                        return None; // u8 ordinal overflow
                    }
                    cursor = pos;
                    last_accept = pos;
                    if first_element {
                        first_element = false;
                    }
                    continue;
                }
                // Tranche 8: a CHOICE element — an Alternative whose arms
                // all unwrap to bare span leaves. The arms chain as a
                // fork/join union; EVERY arm's accept marks the SAME
                // leaf slot (the per-iteration value is one bare span
                // whichever arm matched — the one-marker-per-state law
                // holds: different states, one slot). The plan counts
                // the choice as ONE leaf in lockstep. This lane keeps
                // the flat plan's slot numbering (the json A/B measured
                // routing these shapes through the tranche-9 per-arm
                // machinery as a regression).
                if let Some(arms) = choice_span_arms(grammar, child, 0) {
                    if arms.is_empty() || arms.len() > u8::MAX as usize {
                        return None;
                    }
                    let element_entry = cursor;
                    let join = nfa.node();
                    for &arm in &arms {
                        let (ls, la) = thompson(grammar, arm, &mut nfa, &mut Vec::new())?;
                        element_accepts_of_node.insert(la, leaf_i);
                        nfa.edge(element_entry, Edge::Epsilon, ls);
                        nfa.edge(la, Edge::Epsilon, join);
                    }
                    leaf_i += 1;
                    if first_element {
                        // The first byte seeds the fork: the union's
                        // start subset covers every arm's entry.
                        first_start = element_entry;
                        first_element = false;
                    }
                    last_accept = join;
                    cursor = join;
                    continue;
                }
                // Tranche 9 (the shapes tranche 8's shared-slot lane
                // cannot express): the per-arm CHOICE element — arms
                // may be flat leaf sequences WITH nested choices (the
                // string interior's shape). Every leaf of arm i marks
                // its own slot in the arm's contiguous range; the
                // assembly picks the arm whose LAST leaf slot carries
                // a cut.
                if let Some((join, _plan9, count)) = build_plan9(
                    grammar,
                    child,
                    &mut nfa,
                    cursor,
                    &mut element_accepts_of_node,
                    leaf_i,
                    0,
                ) {
                    leaf_i = leaf_i.saturating_add(count);
                    if first_element {
                        // The first byte seeds the fork: the union's
                        // start subset covers every arm's entry.
                        first_start = cursor;
                        first_element = false;
                    }
                    last_accept = join;
                    cursor = join;
                    continue;
                }
                let mut leaves = Vec::with_capacity(2);
                leaf_chain(grammar, child, &mut leaves, 0)?;
                if leaves.is_empty() || leaves.len() > u8::MAX as usize {
                    return None;
                }
                let (mut cs, mut ca) = (entry, entry);
                for (j, &leaf) in leaves.iter().enumerate() {
                    let (ls, la) = thompson(grammar, leaf, &mut nfa, &mut Vec::new())?;
                    if j == 0 {
                        cs = ls;
                    } else {
                        nfa.edge(ca, Edge::Epsilon, ls);
                    }
                    ca = la;
                    element_accepts_of_node.insert(la, leaf_i);
                    leaf_i += 1;
                }
                nfa.edge(cursor, Edge::Epsilon, cs);
                cursor = ca;
                if first_element {
                    first_start = cs;
                    first_element = false;
                }
                last_accept = ca;
            }
            (first_start, last_accept)
        }
        // Tranche 9: a whole-body CHOICE that tranche 8 cannot express
        // (sequence arms, nested choices) — the per-arm fork/join with
        // per-arm leaf slots, in lockstep with the compiler's plan9_of
        // gate. A bare-arm choice body keeps the plain union (its
        // per-iteration value is ONE whole-span push — no cuts, the
        // tranche-8 lane; the json A/B measured the per-arm machinery
        // on these shapes as a ~50% regression).
        Some(Atom::Alternative { atoms })
            if !atoms.is_empty() && choice_span_arms(grammar, body_idx, 0).is_none() =>
        {
            let (join, _plan9, _count) = build_plan9(
                grammar,
                body_idx,
                &mut nfa,
                entry,
                &mut element_accepts_of_node,
                0,
                0,
            )?;
            (entry, join)
        }
        _ => thompson(grammar, body_idx, &mut nfa, &mut Vec::new())?,
    };
    nfa.edge(entry, Edge::Epsilon, bs);
    nfa.edge(body_accept, Edge::Epsilon, bs); // the loop back
    nfa.edge(body_accept, Edge::Epsilon, exit);
    let mut marked = std::collections::HashSet::new();
    marked.insert(body_accept);
    let (table, iteration_accepts, element_accepts, inner_accepts) = determinize_marked(
        &nfa,
        entry,
        exit,
        &marked,
        &element_accepts_of_node,
        Some(&inner_accepts_of_node),
    );
    Some(LoopDfa {
        table,
        iteration_accepts,
        element_accepts,
        inner_accepts,
    })
}

/// Whether the atom consumes at least one byte on every match path.
fn consumes_min_one(grammar: &Grammar, idx: usize, depth: usize, seen: &mut Vec<usize>) -> bool {
    if depth > 64 {
        return false;
    }
    if seen.contains(&idx) {
        return true; // a cycle back to an in-progress node: assume consuming
    }
    seen.push(idx);
    let ok = (|| {
        let atom = grammar.get_atom(idx)?;
        Some(match atom {
            Atom::Str { pattern } => !pattern.is_empty(),
            Atom::Re { .. } => true,
            Atom::Sequence { atoms } => atoms
                .iter()
                .any(|&c| consumes_min_one(grammar, c, depth + 1, seen)),
            Atom::Alternative { atoms } => atoms
                .iter()
                .all(|&c| consumes_min_one(grammar, c, depth + 1, seen)),
            Atom::Repetition { atom, min, .. } => {
                *min >= 1 && consumes_min_one(grammar, *atom, depth + 1, seen)
            }
            Atom::Named { atom, .. }
            | Atom::Capture { atom, .. }
            | Atom::Scope { atom }
            | Atom::Ignore { atom }
            | Atom::Trivia { atom }
            | Atom::TriviaCapture { atom, .. } => consumes_min_one(grammar, *atom, depth + 1, seen),
            Atom::Entity { atom } => consumes_min_one(grammar, *atom, depth + 1, seen),
            _ => false,
        })
    })();
    seen.pop();
    ok.unwrap_or(false)
}

/// The determinizer's four lanes: the table, the outer iteration
/// accepts, the leaf-boundary markers, the inner-boundary markers.
type MarkedTables = (DfaTable, Vec<bool>, Vec<Option<u8>>, Vec<Option<u8>>);

fn determinize_marked(
    nfa: &Nfa,
    start: u32,
    accept: u32,
    marked: &std::collections::HashSet<u32>,
    elements: &std::collections::HashMap<u32, u8>,
    inner_elements: Option<&std::collections::HashMap<u32, u8>>,
) -> MarkedTables {
    // Epsilon closures per NFA node.
    let mut closures: Vec<Vec<u32>> = vec![Vec::new(); nfa.nodes as usize];
    for node in 0..nfa.nodes {
        let mut stack = vec![node];
        let mut seen = vec![false; nfa.nodes as usize];
        seen[node as usize] = true;
        while let Some(current) = stack.pop() {
            closures[node as usize].push(current);
            for (from, to) in &nfa.eps {
                if *from == current && !seen[*to as usize] {
                    seen[*to as usize] = true;
                    stack.push(*to);
                }
            }
        }
        closures[node as usize].sort_unstable();
    }

    let close = |set: &[u32]| -> Vec<u32> {
        let mut merged: Vec<u32> = set
            .iter()
            .flat_map(|n| closures[*n as usize].iter().copied())
            .collect();
        merged.sort_unstable();
        merged.dedup();
        merged
    };
    let empty_map: std::collections::HashMap<u32, u8> = std::collections::HashMap::new();
    let inner = inner_elements.unwrap_or(&empty_map);

    let mut ids: HashMap<Vec<u32>, u32> = HashMap::new();
    let mut subsets: Vec<Vec<u32>> = Vec::new();
    let mut transitions: Vec<u32> = Vec::new();
    let mut accepts: Vec<bool> = Vec::new();

    let initial = close(&[start]);
    ids.insert(initial.clone(), 0);
    subsets.push(initial);
    accepts.push(subsets[0].contains(&accept));
    transitions.extend(std::iter::repeat_n(u32::MAX, 256));
    let mut iteration_accepts: Vec<bool> = Vec::new();
    iteration_accepts.push(subsets[0].iter().any(|n| marked.contains(n)));
    let mut element_accepts: Vec<Option<u8>> = Vec::new();
    element_accepts.push(subsets[0].iter().find_map(|n| elements.get(n).copied()));
    let mut inner_accepts: Vec<Option<u8>> = Vec::new();
    inner_accepts.push(subsets[0].iter().find_map(|n| inner.get(n).copied()));

    let mut worklist = 0usize;
    while worklist < subsets.len() {
        let subset = subsets[worklist].clone();
        worklist += 1;
        for byte in 0u32..256 {
            let moved: Vec<u32> = subset
                .iter()
                .filter_map(|n| {
                    nfa.byte_edges
                        .iter()
                        .filter(|(from, bits, _)| *from == *n && bits[byte as usize])
                        .map(|(_, _, to)| *to)
                        .next()
                })
                .collect();
            if moved.is_empty() {
                continue;
            }
            let closed = close(&moved);
            let next = match ids.get(&closed) {
                Some(&id) => id,
                None => {
                    let id = subsets.len() as u32;
                    ids.insert(closed.clone(), id);
                    subsets.push(closed);
                    accepts.push(subsets[id as usize].contains(&accept));
                    iteration_accepts.push(subsets[id as usize].iter().any(|n| marked.contains(n)));
                    element_accepts.push(
                        subsets[id as usize]
                            .iter()
                            .find_map(|n| elements.get(n).copied()),
                    );
                    inner_accepts.push(
                        subsets[id as usize]
                            .iter()
                            .find_map(|n| inner.get(n).copied()),
                    );
                    transitions.extend(std::iter::repeat_n(u32::MAX, 256));
                    id
                }
            };
            transitions[(worklist - 1) * 256 + byte as usize] = next;
        }
    }

    let table = DfaTable {
        transitions,
        accepts,
        start: 0,
        state_count: subsets.len() as u32,
    };
    (table, iteration_accepts, element_accepts, inner_accepts)
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::grammar::Grammar;

    /// rule = "ab" — a two-byte chain.
    fn grammar_ab() -> Grammar {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".into(),
        });
        let b = g.add_atom(Atom::Str {
            pattern: "b".into(),
        });
        let body = g.add_atom(Atom::Sequence { atoms: vec![a, b] });
        let rule = g.add_atom(Atom::Named {
            name: "ab".into(),
            atom: body,
        });
        g.root = rule;
        g
    }

    /// rule = ("a" | "b")* — an unbounded loop over disjoint branches.
    fn grammar_ab_star() -> Grammar {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".into(),
        });
        let b = g.add_atom(Atom::Str {
            pattern: "b".into(),
        });
        let alt = g.add_atom(Atom::Alternative { atoms: vec![a, b] });
        let body = g.add_atom(Atom::Repetition {
            atom: alt,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        let rule = g.add_atom(Atom::Named {
            name: "abs".into(),
            atom: body,
        });
        g.root = rule;
        g
    }

    /// Walk the whole input and report the last accept end.
    fn walk(table: &DfaTable, input: &[u8]) -> Option<usize> {
        let mut state = table.start;
        let mut last = if table.is_accept(state) {
            Some(0usize)
        } else {
            None
        };
        for (i, &byte) in input.iter().enumerate() {
            match table.step(state, byte) {
                Some(next) => {
                    state = next;
                    if table.is_accept(state) {
                        last = Some(i + 1);
                    }
                }
                None => break,
            }
        }
        last
    }

    #[test]
    fn builds_the_byte_chain_language() {
        let g = grammar_ab();
        let table = build_rule_dfa(&g, g.root).expect("the chain builds");
        assert_eq!(walk(&table, b"ab"), Some(2));
        assert_eq!(walk(&table, b"ax"), None, "stuck past 'a' — no accept");
        assert_eq!(walk(&table, b"a"), None, "partial — no accept yet");
    }

    #[test]
    fn builds_the_loop_language_greedy() {
        let g = grammar_ab_star();
        let table = build_rule_dfa(&g, g.root).expect("the loop builds");
        assert_eq!(walk(&table, b""), Some(0), "min 0: the empty match accepts");
        assert_eq!(walk(&table, b"abab"), Some(4), "the greedy longest");
        assert_eq!(walk(&table, b"abx"), Some(2), "stops at the stuck byte");
    }

    #[test]
    fn refuses_predicates_and_recursion() {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".into(),
        });
        let la = g.add_atom(Atom::Lookahead {
            atom: a,
            positive: true,
        });
        let body = g.add_atom(Atom::Sequence { atoms: vec![la, a] });
        let rule = g.add_atom(Atom::Named {
            name: "p".into(),
            atom: body,
        });
        g.root = rule;
        assert!(build_rule_dfa(&g, rule).is_none(), "a lookahead refuses");

        let mut g = Grammar::new();
        let slot = g.add_atom(Atom::Sequence { atoms: vec![] });
        let s_x = g.add_atom(Atom::Str {
            pattern: "x".into(),
        });
        let ent = g.add_atom(Atom::Entity { atom: slot });
        let rep = g.add_atom(Atom::Repetition {
            atom: ent,
            min: 0,
            max: Some(1),
            tag: RepetitionTag::Maybe,
        });
        g.atoms[slot] = Atom::Sequence {
            atoms: vec![s_x, rep],
        };
        assert!(build_rule_dfa(&g, slot).is_none(), "recursion refuses");
    }
}

#[cfg(test)]
mod executor_tests {
    use super::*;
    use crate::portable::arena::AstArena;
    use crate::portable::bytecode::instruction::Instruction;
    use crate::portable::bytecode::program::Program;
    use crate::portable::bytecode::vm::parse_with_vm;

    /// The executor-level identity: a program of [DfaRun, End] parses
    /// the same prefix the builder table accepts, consuming exactly the
    /// greedy longest accept and pushing one span.
    #[test]
    fn dfa_run_consumes_the_longest_accept() {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".into(),
        });
        let b = g.add_atom(Atom::Str {
            pattern: "b".into(),
        });
        let alt = g.add_atom(Atom::Alternative { atoms: vec![a, b] });
        let body = g.add_atom(Atom::Repetition {
            atom: alt,
            min: 1,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        let rule = g.add_atom(Atom::Named {
            name: "run".into(),
            atom: body,
        });
        g.root = rule;
        let table = build_rule_dfa(&g, rule).expect("the loop builds");

        let mut program = Program::new();
        let idx = program.add_dfa_table(table);
        program.add_instruction(Instruction::dfa_run(idx));
        program.add_instruction(Instruction::End);

        let mut arena = AstArena::new();
        let result = parse_with_vm(&program, "ababxx", &mut arena).expect("the parse succeeds");
        assert_eq!(result.end_pos, 4, "the greedy longest accept, xx refused");
        let span = result.value;
        let (offset, length) = match span {
            crate::portable::ast::AstNode::InputRef { offset, length } => (offset, length),
            other => panic!("expected the span value, got {other:?}"),
        };
        assert_eq!((offset, length), (0, 4));

        // Refusal: no accept reachable.
        let mut arena = AstArena::new();
        assert!(
            parse_with_vm(&program, "xx", &mut arena).is_err(),
            "no accept state — the rule fails and the parse rejects"
        );
    }
}
