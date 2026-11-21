//! Compiler from Grammar to bytecode Program
//!
//! This module implements the compiler that transforms parsanol's `Grammar`
//! (represented as `Atom` nodes) into a `Program` of bytecode instructions.

use super::instruction::Instruction;
use super::program::{CharSet, Program};
use super::program::{NegScanSpec, NegTerm};
use crate::portable::char_class::{normalize_ranges, CharRanges, CharacterPattern};
use crate::portable::grammar::{Atom, Grammar, RepetitionTag};
use std::collections::{HashMap, HashSet, VecDeque};

/// Placeholder for forward references (will be patched later)
const PLACEHOLDER_OFFSET: i32 = 0;

/// Compiler state
#[derive(Debug)]
struct CompilerState {
    /// Index into the grammar's atoms array being compiled
    current_atom: usize,
}

/// The compiler transforms a Grammar into a bytecode Program
#[derive(Debug)]
pub struct Compiler {
    /// The grammar being compiled
    grammar: Grammar,

    /// The program being built
    program: Program,

    /// Compiler state
    state: CompilerState,

    /// Pending label patches: (instruction index, atom index to jump to)
    pending_patches: Vec<(usize, usize)>,

    /// Nesting depth inside repetition bodies. ByteDispatch tables are a
    /// per-NODE win: inside a repetition the choice runs per ITERATION
    /// (often per character — json's ws = *( %x20 / ... )), where the
    /// table lookup+jump costs more than the interleaved charset probes
    /// (measured 18x on the rs#533 JSON receipt when ws dispatched).
    /// Dispatch is emitted only at depth 0.
    repetition_depth: usize,

    /// Entity-referenced rules whose bodies still need to be compiled as
    /// subroutines after the main code (a call's return address points to
    /// the instruction after the call, so bodies must not sit inline).
    subroutine_queue: VecDeque<usize>,

    /// Recursion guard depth for compile_atom.
    compile_depth: u32,

    /// Atoms that act as rules: every Named atom and every Entity
    /// target. Rule references (shared, possibly cyclic through them)
    /// compile as subroutine calls, never inline.
    rule_atoms: std::collections::HashSet<usize>,

    /// Rules whose subroutine body has been emitted.
    compiled_rules: std::collections::HashSet<usize>,

    /// Per-rule exact FIRST-byte sets + nullability (rs#574's prover):
    /// a fixpoint over the rule graph, so call chains and cycles
    /// resolve instead of failing the inline walk the way a visited
    /// collision did. None = the rule's closure contains an unprovable
    /// atom (Dynamic/State/Lookahead/Lookbehind/Constant/Cut).
    rule_first: HashMap<usize, Option<(Vec<u8>, bool)>>,

    /// Dispatch tables emitted; the peephole optimizer shifts
    /// instruction indices and cannot fix up table offsets.
    emitted_dispatch: bool,

    /// True while emitting a rule's own body (the body itself compiles
    /// inline; only references to OTHER rules become calls).
    compiling_rule_body: bool,
}

impl Compiler {
    /// Create a new compiler for the given grammar
    #[inline]
    pub fn new(grammar: Grammar) -> Self {
        let atom_count = grammar.atoms.len();
        let rule_atoms = Self::discover_rule_atoms(&grammar);
        let rule_first = Self::solve_rule_first(&grammar, &rule_atoms);
        Self {
            grammar,
            program: Program::with_capacity(atom_count * 4, atom_count, atom_count / 4),
            state: CompilerState { current_atom: 0 },
            pending_patches: Vec::new(),
            repetition_depth: 0,
            subroutine_queue: VecDeque::new(),
            compile_depth: 0,
            rule_atoms,
            rule_first,
            compiled_rules: std::collections::HashSet::new(),
            compiling_rule_body: false,
            emitted_dispatch: false,
        }
    }

    /// The prover's per-rule FIRST-byte sets + nullability (rs#574):
    /// exact for call chains and cycles alike — the analysis surface
    /// the deterministic-subset campaign routes on. None = the rule's
    /// closure carries an unprovable atom.
    pub fn rule_first_sets(&self) -> &HashMap<usize, Option<(Vec<u8>, bool)>> {
        &self.rule_first
    }

    /// Whole-rule LR-family provability (rs#574 Phase 2, slice A):
    /// does the rule's recognition lower to a deterministic finite
    /// automaton? Sequences chain, alternatives branch on
    /// pairwise-disjoint FIRST sets, repetitions loop only when their
    /// body's FIRST set is disjoint from what can follow the loop
    /// (the exit ambiguity) and the body consumes at least one byte;
    /// recursion, predicates, and non-terminal constructs refuse with
    /// the reason. The verdicts route slice B's per-rule automaton
    /// emission; the corpus survey publishes the coverage.
    pub fn rule_lr_provability(&self) -> HashMap<usize, LrVerdict> {
        let mut out = HashMap::new();
        for (rule, first) in &self.rule_first {
            let verdict = match first {
                None => LrVerdict::Refused {
                    reason: "the FIRST-set closure is poisoned (non-terminal constructs inside)",
                },
                Some((first_bytes, _)) => {
                    // The grammar's ROOT rule is consumed at end-of-input;
                    // every other rule's usage-site continuation is
                    // over-approximated by its own FIRST set.
                    let follow: &[u8] = if *rule == self.grammar.root {
                        &[]
                    } else {
                        first_bytes
                    };
                    let mut in_progress = Vec::new();
                    match self.lr_states(*rule, follow, &mut in_progress) {
                        Ok(states) => LrVerdict::Provable { states },
                        Err(reason) => LrVerdict::Refused { reason },
                    }
                }
            };
            out.insert(*rule, verdict);
        }
        out
    }

    /// The DFA transition count of one atom under the follow set that
    /// can come after it. `in_progress` holds the walk's open atoms —
    /// a re-entry is recursion, which the automaton family does not
    /// cover (the DFA would be infinite).
    fn lr_states(
        &self,
        idx: usize,
        follow: &[u8],
        in_progress: &mut Vec<usize>,
    ) -> Result<usize, &'static str> {
        if in_progress.contains(&idx) {
            return Err("recursion (the atom closure is not finite)");
        }
        let Some(atom) = self.grammar.get_atom(idx) else {
            return Err("unknown atom index");
        };
        in_progress.push(idx);
        let verdict = match atom {
            Atom::Str { pattern } => Ok(pattern.len().max(1)),
            Atom::Re { pattern } => {
                match regex_syntax::parse(pattern)
                    .ok()
                    .map(|hir| hir_first_union(hir.kind()))
                {
                    Some(Some(_)) => Ok(1),
                    _ => Err("a multi-character or unanalyzable regex"),
                }
            }
            Atom::Sequence { atoms } => {
                // Thread the follow right-to-left: child[i]'s follow =
                // FIRST(child[i+1..]) ∪ (nullable(child[i+1..]) ? the
                // outer follow : ∅) — the loop-exit disjointness inside
                // nested repetitions reads the true continuation.
                let mut total = 0usize;
                let mut child_follow = follow.to_vec();
                for &child in atoms.iter().rev() {
                    let n = self.lr_states(child, &child_follow, in_progress)?;
                    total = total.saturating_add(n);
                    // The next (earlier) child's follow = this child's
                    // FIRST ∪ (this child nullable ? the outer follow : ∅).
                    let (mut first, non_null) = self.atom_first(child);
                    if !non_null {
                        first.extend_from_slice(follow);
                    }
                    child_follow = first;
                }
                Ok(total)
            }
            Atom::Alternative { atoms } => {
                let mut sets = Vec::with_capacity(atoms.len());
                for &branch in atoms {
                    let (first, _) = self.atom_first(branch);
                    sets.push(first);
                }
                for i in 0..sets.len() {
                    for other in &sets[i + 1..] {
                        if sets[i].iter().any(|b| other.contains(b)) {
                            return Err("ordered choice with overlapping FIRST sets");
                        }
                    }
                }
                let mut total = 0usize;
                for (branch, set) in atoms.iter().zip(&sets) {
                    total = total.saturating_add(self.lr_states(*branch, set, in_progress)?);
                }
                Ok(total)
            }
            Atom::Repetition {
                atom, min: _, max, ..
            } => {
                let (body_first, body_non_null) = self.atom_first(*atom);
                // The loop-exit ambiguity: a byte that both starts an
                // iteration and can follow the loop is a shift/reduce
                // conflict — the DFA cannot decide iterate vs exit.
                if body_first.iter().any(|b| follow.contains(b)) {
                    return Err("a repetition body that overlaps what follows it");
                }
                // A nullable body in an unbounded loop never terminates.
                if !body_non_null && max.is_none() {
                    return Err("a nullable repetition body in an unbounded loop");
                }
                if !body_non_null {
                    return Err("a nullable repetition body");
                }
                let mut inner_follow = body_first;
                inner_follow.extend_from_slice(follow);
                let n = self.lr_states(*atom, &inner_follow, in_progress)?;
                Ok(n)
            }
            Atom::Named { atom, .. }
            | Atom::Capture { atom, .. }
            | Atom::Scope { atom }
            | Atom::Ignore { atom }
            | Atom::Trivia { atom }
            | Atom::TriviaCapture { atom, .. } => self.lr_states(*atom, follow, in_progress),
            Atom::Entity { atom } => self.lr_states(*atom, follow, in_progress),
            Atom::Cut => Ok(0),
            Atom::Lookahead { .. } => Err("a lookahead predicate"),
            Atom::Lookbehind { .. } => Err("a lookbehind predicate"),
            Atom::Constant { .. } => Err("a constant-output atom"),
            Atom::Dynamic { .. } | Atom::Custom { .. } | Atom::CustomRef { .. } => {
                Err("a host-computed atom")
            }
            Atom::StateSet { .. } | Atom::StateMatch { .. } | Atom::StateSwitch { .. } => {
                Err("a stateful atom")
            }
        };
        in_progress.pop();
        verdict
    }

    /// FIRST bytes + nullability of one atom (the exact per-atom view
    /// over the fixed-point rule table).
    fn atom_first(&self, idx: usize) -> (Vec<u8>, bool) {
        let mut set = Vec::new();
        let mut non_null = false;
        first_of_atom(
            &self.grammar,
            idx,
            &self.rule_first,
            &mut set,
            &mut non_null,
        );
        (set, non_null)
    }

    /// Rule-boundary discovery: Named atoms wrap grammar rules, and
    /// Entity atoms point at rule bodies. Serializers may emit rule
    /// references as shared inlined atoms (cycles included), so these
    /// boundaries are what makes compilation terminate.
    fn discover_rule_atoms(grammar: &Grammar) -> std::collections::HashSet<usize> {
        let mut rules = std::collections::HashSet::new();
        for (idx, atom) in grammar.atoms.iter().enumerate() {
            match atom {
                Atom::Named { .. } => {
                    rules.insert(idx);
                }
                Atom::Entity { atom: target } => {
                    rules.insert(*target);
                }
                _ => {}
            }
        }
        rules
    }

    /// The per-rule exact FIRST-byte sets + nullability (rs#574's
    /// determinism prover). Phase 1 poisons every rule whose atom
    /// closure reaches an unprovable atom (Dynamic/State/Lookahead/
    /// Lookbehind/Constant/Cut/empty Str — the shapes the inline walk
    /// refused); phase 2 iterates FIRST's equations over the clean
    /// rules to the least fixed point, so call CHAINS and CYCLES
    /// converge (sets only grow, bounded by the 256-byte universe)
    /// instead of failing on the visited collision. The fixpoint is
    /// exact, which keeps dispatch sound: the table routes bytes the
    /// branch provably leads with, and misses none.
    fn solve_rule_first(
        grammar: &Grammar,
        rules: &HashSet<usize>,
    ) -> HashMap<usize, Option<(Vec<u8>, bool)>> {
        // Phase 1: poison, computed at the atom level then propagated
        // to rules (a poisoned body poisons every rule that reaches it,
        // through Entity/Named edges and composites alike).
        let mut poisoned_atom: HashSet<usize> = HashSet::new();
        let mut changed = true;
        while changed {
            changed = false;
            for (idx, atom) in grammar.atoms.iter().enumerate() {
                if poisoned_atom.contains(&idx) {
                    continue;
                }
                let poisons = match atom {
                    Atom::Str { pattern } => pattern.is_empty(),
                    Atom::Re { pattern } => regex_syntax::parse(pattern).is_err(),
                    Atom::Sequence { atoms } | Atom::Alternative { atoms } => {
                        atoms.iter().any(|a| poisoned_atom.contains(a))
                    }
                    Atom::Repetition { atom, .. }
                    | Atom::Ignore { atom }
                    | Atom::Trivia { atom }
                    | Atom::TriviaCapture { atom, .. }
                    | Atom::Capture { atom, .. }
                    | Atom::Scope { atom }
                    | Atom::Named { atom, .. }
                    | Atom::Entity { atom } => poisoned_atom.contains(atom),
                    _ => true,
                };
                if poisons {
                    poisoned_atom.insert(idx);
                    changed = true;
                }
            }
        }
        // Phase 2: the least fixed point over the clean rules.
        // Entries start empty/nullable and grow monotonically.
        let mut table: HashMap<usize, Option<(Vec<u8>, bool)>> = HashMap::new();
        for &rule in rules {
            table.insert(
                rule,
                if poisoned_atom.contains(&rule) {
                    None
                } else {
                    Some((Vec::new(), false))
                },
            );
        }
        let mut sweeps = 0usize;
        let mut moved = true;
        while moved {
            moved = false;
            sweeps += 1;
            if sweeps > 1024 {
                break; // unreachable: monotone growth over a finite universe
            }
            for &rule in rules {
                if poisoned_atom.contains(&rule) {
                    continue;
                }
                let (mut set, mut non_null) = (Vec::new(), false);
                first_of_atom(grammar, rule, &table, &mut set, &mut non_null);
                let changed_entry = match table.get(&rule) {
                    Some(Some((old_set, old_nn))) => *old_set != set || *old_nn != non_null,
                    _ => true,
                };
                if changed_entry {
                    table.insert(rule, Some((set, non_null)));
                    moved = true;
                }
            }
        }
        table
    }

    /// Compile the grammar into a program
    pub fn compile(mut self) -> Result<Program, CompileError> {
        // Compile the root atom
        let entry = self.compile_atom(self.grammar.root)?;

        // Set entry point
        self.program.set_entry_point(entry);

        // Terminate the main path: a successful root parse must stop here
        // instead of falling through into the subroutine bodies below.
        self.program.add_instruction(Instruction::end());

        // Compile referenced rule bodies as subroutines. Nested references
        // enqueue more bodies; each body ends with Return.
        while let Some(atom_idx) = self.subroutine_queue.pop_front() {
            if self.compiled_rules.contains(&atom_idx) {
                continue;
            }
            self.compiled_rules.insert(atom_idx);
            let entry = self.program.instruction_count();
            self.program.add_rule_address(atom_idx, entry);
            let prev = self.compiling_rule_body;
            self.compiling_rule_body = true;
            let result = self.compile_atom(atom_idx);
            self.compiling_rule_body = prev;
            result?;
            self.program.add_instruction(Instruction::ret());
        }

        // Add final End instruction
        self.program.add_instruction(Instruction::end());

        // Patch all forward references
        self.patch_references()?;

        // The peephole pass rewrites and removes instructions, shifting
        // indices; dispatch tables bake absolute-relative offsets at
        // emission and cannot be fixed up afterwards. Dispatch IS the
        // optimization for the programs that use it.
        if !self.emitted_dispatch {
            self.program.optimize();
        }

        // Derive non-serialized metadata (scan plans, dynamic flag)
        // once, after the tables are frozen.
        self.program.derive_metadata();

        Ok(self.program)
    }

    /// Compile a single atom and return the entry instruction index
    fn compile_atom(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
        // Rule atoms compile once, as subroutines; every reference to
        // them is a call. This is what keeps shared/cyclic rule
        // references from recursing the compiler.
        // One-shot: the flag marks exactly the top atom of a rule body
        // being emitted; nested references to other rules must call.
        if self.rule_atoms.contains(&atom_idx) && !std::mem::take(&mut self.compiling_rule_body) {
            return self.compile_reference(atom_idx);
        }

        self.compile_depth += 1;
        if self.compile_depth > 4_096 {
            return Err(CompileError::UnsupportedFeature {
                feature: format!("grammar nesting exceeds compilation depth at atom {atom_idx}"),
            });
        }
        let result = self.compile_atom_inner(atom_idx);
        self.compile_depth -= 1;
        result
    }

    /// Compile an ordered choice whose branches have provably disjoint
    /// non-nullable lead-byte sets: a 256-way table jump selects the
    /// branch directly; a lead byte in no set fails the choice. Each
    /// branch still matches its own lead character, so value semantics
    /// are identical to the interleaved compilation.
    fn compile_dispatch_alternative(
        &mut self,
        atoms: &[usize],
        sets: &[Vec<u8>],
    ) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();
        self.program
            .add_instruction(Instruction::byte_dispatch(u32::MAX));
        let dispatch_idx = entry;

        let mut branch_starts = Vec::with_capacity(atoms.len());
        let mut jump_idxs = Vec::with_capacity(atoms.len().saturating_sub(1));
        for (i, &atom_idx) in atoms.iter().enumerate() {
            branch_starts.push(self.program.instruction_count());
            self.compile_atom(atom_idx)?;
            if i + 1 < atoms.len() {
                // A dispatched branch must not fall into the next one.
                let jump_idx = self.program.instruction_count();
                self.program
                    .add_instruction(Instruction::jump(PLACEHOLDER_OFFSET));
                jump_idxs.push(jump_idx);
            }
        }
        let end_idx = self.program.instruction_count();
        for jump_idx in jump_idxs {
            let offset = (end_idx as i32) - (jump_idx as i32 + 1);
            self.program
                .set_instruction(jump_idx, Instruction::jump(offset));
        }

        let mut table = [-1i32; 256];
        for (branch, set) in sets.iter().enumerate() {
            let offset = (branch_starts[branch] as i32) - (dispatch_idx as i32 + 1);
            for &b in set {
                table[b as usize] = offset;
            }
        }
        let table_idx = self.program.add_dispatch_table(table);
        self.program
            .set_instruction(dispatch_idx, Instruction::byte_dispatch(table_idx));
        self.emitted_dispatch = true;

        Ok(entry)
    }

    /// Detect the shared-prefix shape: every branch is a sequence of
    /// at least two atoms whose head atom index is identical. Returns the
    /// shared head and each branch's tail slice. Index identity is
    /// the sharing test (the serializer shares rule atoms, so common
    /// rule prefixes share their index; structural isomorphism across
    /// distinct indices is not attempted).
    fn shared_prefix_split(&self, atoms: &[usize]) -> Option<(usize, Vec<Vec<usize>>)> {
        if atoms.len() < 2 {
            return None;
        }
        let mut head: Option<usize> = None;
        let mut tails: Vec<Vec<usize>> = Vec::with_capacity(atoms.len());
        for &b in atoms {
            match self.grammar.get_atom(b) {
                Some(Atom::Sequence { atoms: children }) if children.len() >= 2 => {
                    match head {
                        None => head = Some(children[0]),
                        Some(h) if h == children[0] => {}
                        _ => return None,
                    }
                    tails.push(children[1..].to_vec());
                }
                _ => return None,
            }
        }
        head.map(|h| (h, tails))
    }

    /// Compile `alt(seq(H, t1...), seq(H, t2...), ...)` as
    /// `<H> <choice over tails>`: the shared prefix matches once, the
    /// ordered choice (or a dispatch table, when the tails' lead sets
    /// are provably disjoint) runs at the post-prefix position, and
    /// every branch folds its prefix value together with its tail
    /// values via BuildSeq — the same envelope the unsplit branch
    /// sequence produced.
    fn compile_prefix_split_alternative(
        &mut self,
        head: usize,
        tails: &[Vec<usize>],
    ) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();
        self.compile_atom(head)?;

        // Dispatch over tails when provably disjoint; otherwise the
        // ordinary interleaved Choice chain over the same bodies.
        let tail_sets: Option<Vec<Vec<u8>>> = tails
            .iter()
            .map(|t| {
                let mut visited = std::collections::HashSet::new();
                self.provable_first_set_seq(t, &mut visited)
                    .and_then(|(set, non_nullable)| non_nullable.then_some(set))
            })
            .collect();
        let disjoint = tail_sets.as_ref().is_some_and(|sets| {
            for (i, set) in sets.iter().enumerate() {
                for other in sets.iter().skip(i + 1) {
                    if set.iter().any(|b| other.contains(b)) {
                        return false;
                    }
                }
            }
            true
        });

        if disjoint && self.repetition_depth == 0 {
            let dispatch_idx = self.program.instruction_count();
            self.program
                .add_instruction(Instruction::byte_dispatch(u32::MAX));

            let mut branch_starts = Vec::with_capacity(tails.len());
            let mut jump_idxs = Vec::with_capacity(tails.len().saturating_sub(1));
            for (i, tail) in tails.iter().enumerate() {
                branch_starts.push(self.program.instruction_count());
                for &atom_idx in tail {
                    self.compile_atom(atom_idx)?;
                }
                self.program
                    .add_instruction(Instruction::build_seq((1 + tail.len()) as u32));
                if i + 1 < tails.len() {
                    let jump_idx = self.program.instruction_count();
                    self.program
                        .add_instruction(Instruction::jump(PLACEHOLDER_OFFSET));
                    jump_idxs.push(jump_idx);
                }
            }
            let end_idx = self.program.instruction_count();
            for jump_idx in jump_idxs {
                let offset = (end_idx as i32) - (jump_idx as i32 + 1);
                self.program
                    .set_instruction(jump_idx, Instruction::jump(offset));
            }

            let sets = tail_sets.expect("disjoint implies Some");
            let mut table = [-1i32; 256];
            for (branch, set) in sets.iter().enumerate() {
                let offset = (branch_starts[branch] as i32) - (dispatch_idx as i32 + 1);
                for &b in set {
                    table[b as usize] = offset;
                }
            }
            let table_idx = self.program.add_dispatch_table(table);
            self.program
                .set_instruction(dispatch_idx, Instruction::byte_dispatch(table_idx));
            self.emitted_dispatch = true;
        } else {
            // Interleaved Choice chain over tails; every branch pops
            // its Choice at its Commit, and a branch failure lands on
            // the next tail at the post-prefix position.
            let mut choice_idxs = Vec::with_capacity(tails.len() - 1);
            let mut commit_idxs = Vec::with_capacity(tails.len() - 1);
            for (i, tail) in tails.iter().enumerate() {
                if i + 1 < tails.len() {
                    let idx = self.program.instruction_count();
                    self.program
                        .add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));
                    choice_idxs.push(idx);
                }
                for &atom_idx in tail {
                    self.compile_atom(atom_idx)?;
                }
                self.program
                    .add_instruction(Instruction::build_seq((1 + tail.len()) as u32));
                if i + 1 < tails.len() {
                    let idx = self.program.instruction_count();
                    self.program.add_instruction(Instruction::commit(0));
                    commit_idxs.push(idx);
                }
            }
            let after_all = self.program.instruction_count();
            // Choice i's fail-resume is the NEXT tail's unit (the
            // following Choice, or — for the last choice — the final
            // tail's body right after its Commit). Pointing every
            // Choice at the join skipped the remaining tails: a
            // first-alternative failure (e.g. abstractSupertype's
            // `ABSTRACT SUPERTYPE ;` missing its tSUPERTYPE) landed in
            // the rule's epilogue projection and "succeeded" with
            // garbage instead of trying `ABSTRACT ;` (rs#496).
            for (i, &choice_idx) in choice_idxs.iter().enumerate() {
                let next_start = if i + 1 < choice_idxs.len() {
                    choice_idxs[i + 1]
                } else {
                    commit_idxs[i] + 1
                };
                let offset = (next_start as i32) - (choice_idx as i32 + 1);
                self.program
                    .set_instruction(choice_idx, Instruction::choice(offset));
            }
            for &commit_idx in &commit_idxs {
                let offset = (after_all as i32) - (commit_idx as i32 + 1);
                self.program
                    .set_instruction(commit_idx, Instruction::commit(offset));
            }
        }

        Ok(entry)
    }

    /// Emit a call to a rule's (possibly not yet compiled) body and
    /// queue the body for trailing subroutine compilation.
    fn compile_reference(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();
        if let Some(target_addr) = self.program.get_rule_address(atom_idx) {
            let offset = (target_addr as i32) - (entry as i32 + 1);
            self.program.add_instruction(Instruction::call(offset));
        } else {
            self.program
                .add_instruction(Instruction::call(PLACEHOLDER_OFFSET));
            self.pending_patches.push((entry, atom_idx));
            self.subroutine_queue.push_back(atom_idx);
        }
        Ok(entry)
    }

    fn compile_atom_inner(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
        // Get the atom first to check if it's an Entity (rule reference)
        let atom =
            self.grammar
                .get_atom(atom_idx)
                .cloned()
                .ok_or(CompileError::InvalidAtomIndex {
                    index: atom_idx,
                    max: self.grammar.atoms.len(),
                })?;

        // Record the entry point for this atom (primarily for Entity references)
        let entry = self.program.instruction_count();
        self.program.add_rule_address(atom_idx, entry);

        self.state.current_atom = atom_idx;

        match atom {
            Atom::Str { pattern } => self.compile_str(&pattern),
            Atom::Re { pattern } => self.compile_re(&pattern),
            Atom::Sequence { atoms } => self.compile_sequence(&atoms),
            Atom::Alternative { atoms } => self.compile_alternative(&atoms),
            Atom::Repetition {
                atom,
                min,
                max,
                tag,
            } => {
                self.repetition_depth += 1;
                let result = self.compile_repetition(atom, min, max, tag);
                self.repetition_depth -= 1;
                result
            }
            Atom::Named { name, atom } => self.compile_named(&name, atom),
            Atom::Entity { atom } => self.compile_reference(atom),
            Atom::Lookahead { atom, positive } => self.compile_lookahead(atom, positive),
            Atom::Cut => self.compile_cut(),
            Atom::Ignore { atom } | Atom::Trivia { atom } => self.compile_ignore(atom),
            Atom::Capture { name, atom } => self.compile_capture(&name, atom),
            Atom::Scope { atom } => self.compile_scope(atom),
            Atom::Dynamic { callback_id } => self.compile_dynamic(callback_id),
            Atom::Custom { id } => self.compile_custom(id),
            Atom::Constant { .. } | Atom::Lookbehind { .. } => {
                Err(CompileError::UnsupportedFeature {
                    feature: "constant-yield / lookbehind atoms \
                              (parsanol-ruby#137 follow-up); this program tier \
                              declines and the caller runs the tree-walking \
                              interpreter"
                        .to_string(),
                })
            }
            Atom::StateSet { .. }
            | Atom::StateMatch { .. }
            | Atom::StateSwitch { .. }
            | Atom::CustomRef { .. } => Err(CompileError::UnsupportedFeature {
                feature: "ruby-tier state/custom atom (parsanol-ruby#129)".to_string(),
            }),
            Atom::TriviaCapture { .. } => Err(CompileError::UnsupportedFeature {
                feature: "trivia capture needs the interpreter's context channel                           (parsanol-ruby#152); this program tier declines and the                           caller runs the tree-walking interpreter"
                    .to_string(),
            }),
        }
    }

    /// Compile a string literal
    fn compile_str(&mut self, pattern: &str) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        if pattern.is_empty() {
            // Empty string always matches; still produce a zero-width
            // value so every atom leaves exactly one value.
            self.program.add_instruction(Instruction::any(0));
            return Ok(entry);
        }

        if pattern.len() == 1 {
            // Single character: use Char instruction
            self.program
                .add_instruction(Instruction::char(pattern.as_bytes()[0]));
        } else {
            // Multiple characters: use String instruction
            let str_idx = self.program.add_string(pattern);
            self.program
                .add_instruction(Instruction::string(str_idx, pattern.len() as u32));
        }

        Ok(entry)
    }

    /// Compile a regex pattern
    fn compile_re(&mut self, pattern: &str) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // Try to optimize common patterns to character classes
        if let Some(char_pattern) = CharacterPattern::from_pattern(pattern) {
            let set = self.char_pattern_to_set(char_pattern);
            let set_idx = self.program.add_char_set(set);
            self.program.add_instruction(Instruction::charset(set_idx));
            return Ok(entry);
        }

        // ASCII-only character classes become a byte charset
        if let Some(set) = ascii_class_set(pattern) {
            let set_idx = self.program.add_char_set(set);
            self.program.add_instruction(Instruction::charset(set_idx));
            return Ok(entry);
        }

        // Fall back to regex
        let regex_idx = self.program.add_regex(pattern);
        self.program.add_instruction(Instruction::regex(regex_idx));

        Ok(entry)
    }

    /// Convert a CharacterPattern to a CharSet
    fn char_pattern_to_set(&self, pattern: CharacterPattern) -> CharSet {
        use crate::portable::char_class::CHAR_CLASSES;

        let mut set = CharSet::new();

        for b in 0u8..=255u8 {
            let matches = CHAR_CLASSES.matches_pattern(pattern, b);
            if matches {
                set.add(b);
            }
        }

        set
    }

    /// Compile a sequence of atoms
    fn compile_sequence(&mut self, atoms: &[usize]) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // Each child pushes one value; fold them into a :sequence-tagged
        // envelope exactly like the tree-walker's parse_sequence.
        for &atom_idx in atoms {
            self.compile_atom(atom_idx)?;
        }
        self.program
            .add_instruction(Instruction::build_seq(atoms.len() as u32));

        Ok(entry)
    }

    /// Conservative first-byte analysis for dispatch compilation.
    /// Returns (charset, provably_non_nullable) or None when the lead
    /// byte cannot be proven — in which case dispatch is not emitted.
    fn provable_first_set(
        &self,
        atom_idx: usize,
        visited: &mut std::collections::HashSet<usize>,
    ) -> Option<(Vec<u8>, bool)> {
        if !visited.insert(atom_idx) {
            return None; // cycle: unprovable
        }
        let result = match self.grammar.get_atom(atom_idx)? {
            Atom::Str { pattern } => {
                let bytes = pattern.as_bytes();
                if bytes.is_empty() {
                    None
                } else {
                    Some((vec![bytes[0]], true))
                }
            }
            Atom::Re { pattern } => self.regex_first_bytes(pattern).map(|cs| (cs, true)),
            Atom::Sequence { atoms } => self.provable_first_set_seq(atoms, visited),
            Atom::Alternative { atoms } => {
                let mut union: Vec<u8> = Vec::new();
                let mut all_non_nullable = true;
                for &child in atoms {
                    let (set, non_nullable) = self.provable_first_set(child, visited)?;
                    all_non_nullable &= non_nullable;
                    for b in set {
                        if !union.contains(&b) {
                            union.push(b);
                        }
                    }
                }
                Some((union, all_non_nullable))
            }
            Atom::Repetition { atom, min, .. } => {
                let (set, non_nullable) = self.provable_first_set(*atom, visited)?;
                Some((set, non_nullable && *min >= 1))
            }
            // Rule references stay INLINE-walked for DISPATCH
            // eligibility: the fixpoint prover (rs#574) resolves their
            // exact sets, but wiring them into emission measured a 13%
            // loss on json (the root value choice dispatches per VALUE,
            // where interleaved fail-probes are cheaper than the table
            // hop). The prover is the analysis surface; the emission
            // gate reopens per shape with its own A/B receipt.
            Atom::Named { atom, .. }
            | Atom::Entity { atom }
            | Atom::Ignore { atom }
            | Atom::Trivia { atom }
            | Atom::TriviaCapture { atom, .. }
            | Atom::Capture { atom, .. }
            | Atom::Scope { atom } => self.provable_first_set(*atom, visited),
            Atom::Lookahead { .. } | Atom::Cut | Atom::Dynamic { .. } | Atom::Custom { .. } => None,
            Atom::Constant { .. } | Atom::Lookbehind { .. } => None,
            // Ruby-tier state atoms: unprovable (state-dependent or
            // foreign). An inline-expression set COULD recurse into the
            // expr's first set, but the state write itself is a side
            // effect the bytecode compiler does not model.
            Atom::StateSet { .. }
            | Atom::StateMatch { .. }
            | Atom::StateSwitch { .. }
            | Atom::CustomRef { .. } => None,
        };
        visited.remove(&atom_idx);
        result
    }

    /// First-byte analysis over a sequence slice (used both for
    /// Sequence atoms and for the tail of a shared-prefix split).
    /// Union of leading nullable children's sets, then the first
    /// provably non-nullable child's set. A nullable child CAN
    /// consume its lead byte, so its set must be part of the union
    /// for ordering to stay sound.
    fn provable_first_set_seq(
        &self,
        atoms: &[usize],
        visited: &mut std::collections::HashSet<usize>,
    ) -> Option<(Vec<u8>, bool)> {
        let mut union: Vec<u8> = Vec::new();
        for &child in atoms {
            let (set, non_nullable) = self.provable_first_set(child, visited)?;
            for b in set {
                if !union.contains(&b) {
                    union.push(b);
                }
            }
            if non_nullable {
                return Some((union, true));
            }
        }
        Some((union, false))
    }

    /// The exact single byte a branch atom matches, when the branch is
    /// a bare one-byte match: Str of one byte, or an Re decoding to a
    /// class that matches exactly one byte (probed over all 256 —
    /// multi-byte codepoints fail the one-byte haystack). Anything
    /// wrapped (Named/Capture/...) or multi-byte returns None.
    fn single_byte_atom(&self, atom_idx: usize) -> Option<u8> {
        match self.grammar.get_atom(atom_idx)? {
            Atom::Str { pattern } if pattern.len() == 1 => Some(pattern.as_bytes()[0]),
            Atom::Re { pattern } => {
                let ranges = crate::portable::char_class::CharRanges::from_pattern(pattern)?;
                let mut found: Option<u8> = None;
                for b in 0..=255u8 {
                    if ranges.match_at(&[b], 0) == Some(1) {
                        if found.is_some() {
                            return None;
                        }
                        found = Some(b);
                    }
                }
                found
            }
            _ => None,
        }
    }

    /// The merged charset for an alternative whose branches are all
    /// bare distinct single-byte matches, or None.
    fn single_byte_choice_set(&self, atoms: &[usize]) -> Option<CharSet> {
        if atoms.len() < 2 {
            return None;
        }
        let mut set = CharSet::new();
        for &a in atoms {
            let b = self.single_byte_atom(a)?;
            if set.contains(b) {
                return None; // overlapping branches: ordered choice semantics
            }
            set.add(b);
        }
        Some(set)
    }

    /// Lead bytes of a regex we can prove: a character class, a literal,
    /// or an alternation/group of provable shapes. Case-insensitive
    /// literals lower to `(?i:...)` groups whose HIR is an alternation of
    /// case-folded literal strings — json's value choice needs their
    /// first bytes ({t,T}, {f,F}, {n,N}) to dispatch.
    fn regex_first_bytes(&self, pattern: &str) -> Option<Vec<u8>> {
        let hir = regex_syntax::parse(pattern).ok()?;
        let mut union: Vec<u8> = Vec::new();
        self.hir_first_bytes(hir.kind(), &mut union)?;
        Some(union)
    }

    /// Accumulate every byte the HIR node can START a match with;
    /// `None` when the shape is unprovable (anchors, repetitions,
    /// lookarounds, empty matches).
    fn hir_first_bytes(
        &self,
        kind: &regex_syntax::hir::HirKind,
        union: &mut Vec<u8>,
    ) -> Option<()> {
        use regex_syntax::hir::{Class, HirKind};
        match kind {
            HirKind::Class(Class::Unicode(class)) => {
                for item in class.iter() {
                    let (lo, hi) = (item.start() as u32, item.end() as u32);
                    for b in utf8_first_bytes_range(lo, hi) {
                        if !union.contains(&b) {
                            union.push(b);
                        }
                    }
                }
                Some(())
            }
            HirKind::Literal(lit) => {
                let s = std::str::from_utf8(&lit.0).ok()?;
                let first = s.chars().next()?;
                for b in utf8_first_bytes_range(first as u32, first as u32) {
                    if !union.contains(&b) {
                        union.push(b);
                    }
                }
                Some(())
            }
            HirKind::Alternation(branches) => {
                for branch in branches {
                    self.hir_first_bytes(branch.kind(), union)?;
                }
                Some(())
            }
            // A concat contributes its FIRST child's lead bytes (classes
            // and literals are non-nullable, so later children cannot
            // start the match). (?i:true) lowers exactly to
            // Concat[Class[tT], Class[rR], Class[uU], Class[eE]].
            HirKind::Concat(children) => {
                let first = children.first()?;
                self.hir_first_bytes(first.kind(), union)
            }
            HirKind::Capture(cap) => self.hir_first_bytes(cap.sub.kind(), union),
            _ => None,
        }
    }

    /// Compile alternatives (ordered choice)
    fn compile_alternative(&mut self, atoms: &[usize]) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        if atoms.is_empty() {
            return Ok(entry);
        }

        if atoms.len() == 1 {
            return self.compile_atom(atoms[0]);
        }

        // Single-byte merge: every branch a bare single-byte match with
        // pairwise-distinct bytes compiles to ONE charset — the same
        // byte is consumed and the same span value produced whichever
        // branch matches, so acceptance and trees are identical by
        // construction. This is the per-character choice shape (json's
        // ws = *( %x20 / %x09 / %x0A / %x0D )), where the interleaved
        // choice costs a fail-branch per character; unlike ByteDispatch
        // it is sound at any repetition depth (same consumed bytes, no
        // per-alternative values to preserve).
        if let Some(set) = self.single_byte_choice_set(atoms) {
            let set_idx = self.program.add_char_set(set);
            self.program.add_instruction(Instruction::charset(set_idx));
            return Ok(entry);
        }

        // Shared-prefix split (TODO.perf/3): when every branch is a
        // sequence starting with the same atom, match that prefix once
        // and choose among the tails. Deterministic PEG prefixes make
        // this acceptance-equivalent, and each branch rebuilds its
        // original value envelope (prefix value + tail values), so
        // trees are identical to the unsplit compilation. This is
        // also what unblocks lead-byte dispatch for such alternatives:
        // the shared prefix's bytes polluted every branch's union,
        // which measured as 35 of 63 alternatives blocked in EXPRESS.
        if let Some((head, tails)) = self.shared_prefix_split(atoms) {
            return self.compile_prefix_split_alternative(head, &tails);
        }

        // Lead-byte dispatch: when every branch's first byte set is
        // provable, non-nullable, and pairwise disjoint, one table
        // lookup replaces the serial probing of branches (the EXPRESS
        // grammar's choice-heavy structure measures ~16 backtracks per
        // byte without it).
        // Conservative dispatch: EVERY branch must be provably
        // non-nullable with a pairwise-disjoint lead-byte set. Nullable
        // branches (which can match empty at any byte and must be tried
        // first) and overlaps fall back to the interleaved choice. A
        // lead byte in no set fails the choice outright.
        let analyzed: Option<Vec<Vec<u8>>> = atoms
            .iter()
            .map(|&a| {
                let mut visited = std::collections::HashSet::new();
                self.provable_first_set(a, &mut visited)
                    .and_then(|(set, non_nullable)| non_nullable.then_some(set))
            })
            .collect();
        if let Some(sets) = analyzed {
            let mut disjoint = true;
            'outer: for (i, set) in sets.iter().enumerate() {
                for other in sets.iter().skip(i + 1) {
                    if set.iter().any(|b| other.contains(b)) {
                        disjoint = false;
                        break 'outer;
                    }
                }
            }
            if disjoint {
                // The depth gate (rs#533: dispatch measured 18x slower
                // on json's per-character ws choice inside a rep) is
                // absorbed upstream: single-byte choices merge to a
                // charset before this point at ANY depth. What reaches
                // here inside a repetition = multi-byte-branched
                // choices (json's value list), where the table wins.
                // Re-measured on the 1.5.8 receipts before landing.
                return self.compile_dispatch_alternative(atoms, &sets);
            }
        }

        // Ordered choice, LPeg-interleaved: each Choice sits directly
        // before its own branch, so a branch failure pops exactly that
        // Choice and lands on the NEXT branch, and a branch success pops
        // the same Choice at its Commit:
        //
        //   Choice L2     ; try branch 1
        //   <p1>
        //   Commit End    ; branch 1 succeeded, skip the rest
        //   L2: Choice L3 ; try branch 2
        //   <p2>
        //   Commit End
        //   ...
        //   Ln: <pn>      ; last branch, no Choice needed
        //   End:
        let mut choice_idxs = Vec::with_capacity(atoms.len() - 1);
        let mut commit_idxs = Vec::with_capacity(atoms.len() - 1);
        for (i, &atom_idx) in atoms.iter().enumerate() {
            if i < atoms.len() - 1 {
                let idx = self.program.instruction_count();
                self.program
                    .add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));
                choice_idxs.push(idx);
            }
            self.compile_atom(atom_idx)?;
            if i < atoms.len() - 1 {
                let idx = self.program.instruction_count();
                self.program
                    .add_instruction(Instruction::commit(PLACEHOLDER_OFFSET));
                commit_idxs.push(idx);
            }
        }

        let end_idx = self.program.instruction_count();

        // Choice i's alternative is the Choice (or last branch) that
        // follows branch i's Commit.
        for (i, &choice_idx) in choice_idxs.iter().enumerate() {
            let next_start = if i + 1 < choice_idxs.len() {
                choice_idxs[i + 1]
            } else {
                commit_idxs[i] + 1
            };
            let offset = (next_start as i32) - (choice_idx as i32 + 1);
            self.program
                .set_instruction(choice_idx, Instruction::choice(offset));
        }

        // Every Commit skips the remaining branches.
        for &commit_idx in &commit_idxs {
            let offset = (end_idx as i32) - (commit_idx as i32 + 1);
            self.program
                .set_instruction(commit_idx, Instruction::commit(offset));
        }

        Ok(entry)
    }

    /// Compile repetition (min, max)
    #[allow(clippy::too_many_arguments)]
    fn compile_repetition(
        &mut self,
        atom_idx: usize,
        min: usize,
        max: Option<usize>,
        tag: RepetitionTag,
    ) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();
        let maybe = tag == RepetitionTag::Maybe;

        // The DFA routing (rs#574 slice B stages 2+3): an unlimited
        // plain repetition whose body leaves one SPAN-shaped value per
        // iteration lowers to ONE self-contained DfaRun. Stage 2: the
        // body = a single single-value terminal (a Str — one
        // String/Char instruction — or an ASCII-class Re). Stage 3: a
        // FLAT Sequence of such terminals (one `:sequence`-tagged
        // array per iteration, the classic BuildSeq fold shape) and a
        // Named-wrapped span terminal / flat span sequence (one
        // `{name: inner}` hash per iteration, the classic BuildHash
        // shape). Capture/Scope/Trivia wrappers stay on the classic
        // loop — they ride the capture machinery, which the routed
        // executor does not run — as do nested multi-value fragments
        // (their synthesis plan is a tree, a later slice).
        if max.is_none() && !maybe && matches!(tag, RepetitionTag::Repetition) {
            // A SPAN TERMINAL (routes bare); the wrapper decides the
            // per-iteration value shape.
            let is_span_terminal = |idx: usize| match self.grammar.get_atom(idx) {
                Some(Atom::Str { pattern }) => !pattern.is_empty(),
                Some(Atom::Re { pattern }) => {
                    crate::portable::char_class::CharRanges::from_pattern(pattern).is_some()
                }
                _ => false,
            };
            let body_shape = match self.grammar.get_atom(atom_idx) {
                Some(Atom::Str { .. }) | Some(Atom::Re { .. }) if is_span_terminal(atom_idx) => {
                    Some((false, None))
                }
                Some(Atom::Sequence { atoms })
                    if !atoms.is_empty() && atoms.iter().all(|&c| is_span_terminal(c)) =>
                {
                    Some((true, None))
                }
                Some(Atom::Named { name, atom }) => match self.grammar.get_atom(*atom) {
                    Some(Atom::Str { .. }) | Some(Atom::Re { .. }) if is_span_terminal(*atom) => {
                        Some((false, Some(name.clone())))
                    }
                    Some(Atom::Sequence { atoms })
                        if !atoms.is_empty() && atoms.iter().all(|&c| is_span_terminal(c)) =>
                    {
                        Some((true, Some(name.clone())))
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some((seq_wrapped, named)) = body_shape {
                if let Some((table, iteration_accepts)) =
                    super::dfa_build::build_loop_dfa(&self.grammar, atom_idx)
                {
                    let idx = self
                        .program
                        .add_dfa_loop_table(table, iteration_accepts, min as u32);
                    if seq_wrapped {
                        self.program.mark_dfa_loop_sequence(idx);
                    }
                    if let Some(name) = named {
                        self.program.mark_dfa_loop_named(idx, name);
                    }
                    self.program.add_instruction(Instruction::dfa_run(idx));
                    return Ok(entry);
                }
            }
        }

        // The tree-walker collects one value per iteration into a tagged
        // envelope; a present Maybe flattens to its single value.
        self.program.add_instruction(Instruction::rep_open());

        if min == 0 && max == Some(0) {
            self.program.add_instruction(Instruction::build_rep(maybe));
            return Ok(entry);
        }

        // The quoted-string idiom `*(!delim terminal)` fuses to one
        // instruction (min 0 only — the mandatory loop keeps exact
        // failure bookkeeping). Per-iteration values are the body's
        // tagged `[Nil, span]` sequence, so the tree is identical.
        if min == 0 {
            if let Some(spec) = self.try_get_neg_scan_spec(atom_idx) {
                let idx = self.program.add_neg_spec(spec);
                let max_u32 = max.map(|m| m as u32).unwrap_or(u32::MAX);
                self.program
                    .add_instruction(Instruction::scan_until(idx, max_u32));
                self.program.add_instruction(Instruction::build_rep(maybe));
                return Ok(entry);
            }
        }

        // Mandatory iterations: failure propagates naturally.
        for _ in 0..min {
            self.compile_atom(atom_idx)?;
            self.program.add_instruction(Instruction::rep_count());
        }

        match max {
            None => {
                // Unlimited: loop with Choice/PartialCommit. PartialCommit
                // must jump to the BODY, not the Choice - the frame is kept
                // and updated, so a later failure keeps completed iterations.
                if let Some(set_idx) = self.try_get_charset_for_atom(atom_idx) {
                    // Character-class body: a single Span matches the whole run.
                    self.program.add_instruction(Instruction::span(set_idx));
                    self.program.add_instruction(Instruction::rep_count());
                } else if let Some(ranges) = self.try_get_class_ranges_for_atom(atom_idx) {
                    // Class-choice body (multi-byte ranges included):
                    // the bulk ScanClass run replaces the per-char
                    // Choice loop — exact codepoint checks, per-char
                    // span values, identical trees by construction.
                    let idx = self.program.add_char_ranges(ranges);
                    // No RepCount after the run: the instruction owns
                    // the register (it counts per character, exactly
                    // like the choice loop it replaces).
                    self.program.add_instruction(Instruction::scan_class(idx));
                } else {
                    let choice_idx = self.program.instruction_count();
                    self.program
                        .add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));
                    let body_start = self.program.instruction_count();
                    self.compile_atom(atom_idx)?;
                    self.program.add_instruction(Instruction::rep_count());
                    let partial_idx = self.program.instruction_count();
                    let loop_offset = (body_start as i32) - (partial_idx as i32 + 1);
                    self.program
                        .add_instruction(Instruction::partial_commit(loop_offset));

                    let after_loop = self.program.instruction_count();
                    let choice_offset = (after_loop as i32) - (choice_idx as i32 + 1);
                    self.program
                        .set_instruction(choice_idx, Instruction::choice(choice_offset));
                }
            }
            Some(max_val) => {
                // Bounded: one Choice per optional iteration; each Commit
                // pops its frame on success, keeping the iteration's value.
                let mut choice_idxs = Vec::new();
                for _ in 0..(max_val - min) {
                    let choice_idx = self.program.instruction_count();
                    self.program
                        .add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));
                    choice_idxs.push(choice_idx);
                    self.compile_atom(atom_idx)?;
                    self.program.add_instruction(Instruction::rep_count());
                    self.program.add_instruction(Instruction::commit(0));
                }
                let after_all = self.program.instruction_count();
                for choice_idx in choice_idxs {
                    let choice_offset = (after_all as i32) - (choice_idx as i32 + 1);
                    self.program
                        .set_instruction(choice_idx, Instruction::choice(choice_offset));
                }
            }
        }

        self.program.add_instruction(Instruction::build_rep(maybe));

        Ok(entry)
    }

    /// Follow Entity chains to the underlying atom.
    fn resolved_atom(&self, idx: usize) -> Option<&Atom> {
        let mut cur = idx;
        for _ in 0..64 {
            match self.grammar.get_atom(cur)? {
                Atom::Entity { atom } => cur = *atom,
                a => return Some(a),
            }
        }
        None
    }

    /// The delimiter strings of a negative lookahead: a Str, a
    /// single-codepoint class (the compiled shape of `!"x"`), or an
    /// Alternative of those, entities followed.
    fn delim_strings_of(&self, atom_idx: usize) -> Option<Vec<String>> {
        fn one_delim(atom: Option<&Atom>) -> Option<String> {
            match atom? {
                Atom::Str { pattern } => Some(pattern.clone()),
                Atom::Re { pattern } => {
                    // `!"x"` compiles to a one-member class; a
                    // multi-codepoint class is not a string delimiter.
                    let ranges = CharRanges::from_pattern(pattern)?;
                    let cps = ranges.codepoint_ranges();
                    if cps.len() == 1 && cps[0].0 == cps[0].1 {
                        char::from_u32(cps[0].0).map(|c| c.to_string())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        match self.resolved_atom(atom_idx)? {
            Atom::Str { .. } | Atom::Re { .. } => {
                Some(vec![one_delim(self.resolved_atom(atom_idx))?])
            }
            Atom::Alternative { atoms } => {
                let mut out = Vec::with_capacity(atoms.len());
                for &a in atoms {
                    out.push(one_delim(self.resolved_atom(a))?);
                }
                Some(out)
            }
            _ => None,
        }
    }

    /// The per-iteration terminal of the quoted-string idiom.
    fn neg_terminal_of(&self, atom_idx: usize) -> Option<NegTerm> {
        match self.resolved_atom(atom_idx)? {
            Atom::Str { pattern } => Some(NegTerm::Lit(pattern.clone())),
            Atom::Re { pattern } => {
                if let Some(p) = CharacterPattern::from_pattern(pattern) {
                    Some(NegTerm::CharPat(p))
                } else {
                    Some(NegTerm::Ranges(
                        (*crate::portable::regex_cache::get_or_decode_class(pattern)?).clone(),
                    ))
                }
            }
            _ => None,
        }
    }

    /// Recognize `Sequence[Lookahead(negative, Str…), terminal]` (the
    /// quoted-string idiom's body); None = the general loop runs.
    fn try_get_neg_scan_spec(&self, atom_idx: usize) -> Option<NegScanSpec> {
        let atoms = match self.grammar.get_atom(atom_idx)? {
            Atom::Sequence { atoms } if atoms.len() == 2 => atoms.clone(),
            _ => return None,
        };
        match self.grammar.get_atom(atoms[0])? {
            Atom::Lookahead {
                atom: la,
                positive: false,
            } => {
                let delims = self.delim_strings_of(*la)?;
                let term = self.neg_terminal_of(atoms[1])?;
                Some(NegScanSpec { delims, term })
            }
            _ => None,
        }
    }

    /// Exact codepoint ranges for an atom when it is a single
    /// codepoint class or a choice of them — the ScanClass fast
    /// path. Ordered-choice semantics collapse into the union: every
    /// branch is a bare single-char match producing the same span
    /// value, so acceptance and trees are identical by construction.
    fn try_get_class_ranges_for_atom(&mut self, atom_idx: usize) -> Option<Vec<(u32, u32)>> {
        let ranges = self.class_ranges_union(atom_idx)?;
        // ASCII-only unions belong to the Span/charset paths: the
        // byte-set match is faster than the decode + range check, and
        // the existing extractors already own every shape they can
        // express (the union flattens nested alternatives the charset
        // extractor refuses). Taking such a union REGRESSED hot ASCII
        // loops on json (a per-char choice over three digit sets
        // became a slower decode loop). ScanClass = bodies with
        // non-ASCII members only.
        if ranges.iter().all(|&(_, hi)| hi <= 0x7F) {
            return None;
        }
        Some(ranges)
    }

    /// The codepoint union for a class/choice atom WITHOUT the ASCII
    /// guard (the guard is a REP-BODY decision, not a per-branch one —
    /// a union mixing ASCII and non-ASCII branches must survive the
    /// recursion).
    fn class_ranges_union(&self, atom_idx: usize) -> Option<Vec<(u32, u32)>> {
        match self.grammar.get_atom(atom_idx)? {
            Atom::Re { pattern } => Some(CharRanges::from_pattern(pattern)?.codepoint_ranges()),
            Atom::Str { pattern } => {
                let mut chars = pattern.chars();
                let (c, extra) = (chars.next()?, chars.next());
                extra.is_none().then(|| vec![(c as u32, c as u32)])
            }
            Atom::Alternative { atoms } => {
                let children: Vec<usize> = atoms.clone();
                let mut union: Vec<(u32, u32)> = Vec::new();
                for &a in &children {
                    union.extend(self.class_ranges_union(a)?);
                }
                Some(normalize_ranges(union))
            }
            _ => None,
        }
    }

    /// Try to get the charset index for an atom if it's a simple charset
    fn try_get_charset_for_atom(&mut self, atom_idx: usize) -> Option<u32> {
        let atom = self.grammar.get_atom(atom_idx)?;

        match atom {
            Atom::Re { pattern } => {
                if let Some(char_pattern) = CharacterPattern::from_pattern(pattern) {
                    let set = self.char_pattern_to_set(char_pattern);
                    Some(self.program.add_char_set(set))
                } else {
                    let set = ascii_class_set(pattern)?;
                    Some(self.program.add_char_set(set))
                }
            }
            Atom::Str { pattern } if pattern.len() == 1 => {
                let mut set = CharSet::new();
                set.add(pattern.as_bytes()[0]);
                Some(self.program.add_char_set(set))
            }
            Atom::Alternative { atoms } => {
                // A choice of bare distinct single-byte matches consumes
                // the same run as one charset of the union (the strict
                // extractor refuses wrapped atoms and overlapping
                // bytes, so ordered-choice semantics are untouched) —
                // the Span fast path applies with the union. This is
                // json's ws shape: *( %x20 / %x09 / %x0A / %x0D ).
                self.single_byte_choice_set(atoms)
                    .map(|set| self.program.add_char_set(set))
            }
            _ => None,
        }
    }

    /// Compile a named capture
    fn compile_named(&mut self, name: &str, atom_idx: usize) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // The tree-walker wraps the child value as {name: value}.
        let name_idx = self.program.add_string(name);
        self.compile_atom(atom_idx)?;
        self.program
            .add_instruction(Instruction::build_hash(name_idx));

        Ok(entry)
    }
    /// Compile lookahead (positive or negative)
    fn compile_lookahead(
        &mut self,
        atom_idx: usize,
        positive: bool,
    ) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        if positive {
            // Positive lookahead: match but don't consume input
            // Structure: Choice fail, <atom>, BackCommit continue, Fail
            // fail: Fail
            // continue: (next instruction after lookahead)
            //
            // How it works:
            // 1. Choice saves current position and sets backtrack target to Fail
            // 2. <atom> is executed (may consume input)
            // 3. If <atom> succeeds: BackCommit restores position and continues
            // 4. If <atom> fails: Choice backtracks to Fail

            let choice_idx = self.program.instruction_count();
            self.program
                .add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));

            self.compile_atom(atom_idx)?;

            let backcommit_idx = self.program.instruction_count();
            self.program.add_instruction(Instruction::back_commit(0)); // Continue to next instruction

            let fail_instr_idx = self.program.instruction_count();
            self.program.add_instruction(Instruction::fail());

            let continue_idx = self.program.instruction_count();

            // Patch choice to jump to fail
            let choice_offset = (fail_instr_idx as i32) - (choice_idx as i32 + 1);
            self.program
                .set_instruction(choice_idx, Instruction::choice(choice_offset));

            // Patch backcommit to continue (offset 0 = next instruction)
            // Actually BackCommit needs to jump PAST the Fail instruction
            let backcommit_offset = (continue_idx as i32) - (backcommit_idx as i32 + 1);
            self.program
                .set_instruction(backcommit_idx, Instruction::back_commit(backcommit_offset));
        } else {
            // Negative lookahead: fail if matches, succeed if doesn't
            // Structure: Choice success, <atom>, FailTwice
            // success: (next instruction after lookahead)
            //
            // How it works:
            // 1. Choice saves current position and sets backtrack target to success
            // 2. <atom> is executed
            // 3. If <atom> succeeds: FailTwice pops choice and fails
            // 4. If <atom> fails: Choice backtracks to success (position restored)

            let choice_idx = self.program.instruction_count();
            self.program
                .add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));

            self.compile_atom(atom_idx)?;
            self.program.add_instruction(Instruction::fail_twice());

            let success_idx = self.program.instruction_count();

            // Patch choice to jump to success
            let choice_offset = (success_idx as i32) - (choice_idx as i32 + 1);
            self.program
                .set_instruction(choice_idx, Instruction::choice(choice_offset));
        }

        // The tree-walker yields nil for lookahead results. The body's
        // value is already gone (BackCommit truncates it), so this pushes
        // rather than replaces.
        self.program.add_instruction(Instruction::push_nil());

        Ok(entry)
    }

    /// Compile cut (atomic predicate)
    fn compile_cut(&mut self) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // The tree-walker's Cut always succeeds consuming nothing and
        // yields nil (the commit semantics live in the Ruby engine).
        self.program.add_instruction(Instruction::push_nil());

        Ok(entry)
    }

    /// Compile ignore (match but discard result)
    fn compile_ignore(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // The tree-walker discards the child's value and yields nil.
        self.compile_atom(atom_idx)?;
        self.program.add_instruction(Instruction::to_nil());

        Ok(entry)
    }

    /// Compile a capture atom
    ///
    /// Captures the matched text with a name for later reference.
    fn compile_capture(&mut self, name: &str, atom_idx: usize) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // The tree-walker records the (name, span) pair and passes the
        // child's value through unchanged.
        let name_idx = self.program.add_string(name);
        self.program.add_instruction(Instruction::cap_mark());
        self.compile_atom(atom_idx)?;
        self.program
            .add_instruction(Instruction::record_capture(name_idx));

        Ok(entry)
    }

    /// Compile a scope atom
    ///
    /// Creates an isolated capture scope. Captures inside are discarded on scope exit.
    fn compile_scope(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // Mark the region; ScopeEnd discards captures recorded inside it,
        // mirroring the tree-walker's CaptureState push/pop.
        self.program.add_instruction(Instruction::cap_mark());
        self.compile_atom(atom_idx)?;
        self.program.add_instruction(Instruction::scope_end());

        Ok(entry)
    }

    /// Compile a dynamic atom
    ///
    /// Invokes a callback at runtime to determine which atom to parse.
    fn compile_dynamic(&mut self, callback_id: u64) -> Result<usize, CompileError> {
        // TODO.perf/5: the VM executes Dynamic atoms by suspending to
        // the callback at runtime (InvokeDynamic delegates to the
        // packrat engine for the resolved fragment, under the shared
        // recursion/budget guards). Rule-call memoization is disabled
        // for programs containing this instruction (see
        // Program::has_invoke_dynamic): dynamic outcomes are
        // capture-dependent, which the memo key ignores.
        let entry = self.program.instruction_count();
        self.program
            .add_instruction(Instruction::InvokeDynamic { callback_id });
        Ok(entry)
    }

    /// Compile custom atom
    fn compile_custom(&mut self, id: u64) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // Emit a Custom instruction that will be handled at runtime
        // The custom atom registry will be looked up at parse time
        self.program.add_instruction(Instruction::custom(id));

        Ok(entry)
    }

    /// Patch all forward references
    fn patch_references(&mut self) -> Result<(), CompileError> {
        for (instr_idx, atom_idx) in self.pending_patches.drain(..) {
            let target_addr = self.program.get_rule_address(atom_idx).ok_or(
                CompileError::UnresolvedReference {
                    atom: atom_idx,
                    from: instr_idx,
                },
            )?;

            let offset = (target_addr as i32) - (instr_idx as i32 + 1);

            // Get the instruction and update its offset
            if let Some(instr) = self.program.get_instruction(instr_idx) {
                let new_instr = match instr {
                    Instruction::Call { .. } => Instruction::call(offset),
                    Instruction::Jump { .. } => Instruction::jump(offset),
                    Instruction::Choice { .. } => Instruction::choice(offset),
                    _ => {
                        return Err(CompileError::Internal {
                            message: format!(
                                "Unexpected instruction type for patching at {}",
                                instr_idx
                            ),
                        })
                    }
                };
                self.program.set_instruction(instr_idx, new_instr);
            }
        }

        Ok(())
    }
}

/// Compilation error
#[derive(Debug, Clone)]
pub enum CompileError {
    /// Invalid atom index
    InvalidAtomIndex {
        /// The invalid index
        index: usize,
        /// Maximum valid index
        max: usize,
    },

    /// Unresolved forward reference
    UnresolvedReference {
        /// The referenced atom
        atom: usize,
        /// The instruction with the reference
        from: usize,
    },

    /// Unsupported feature
    UnsupportedFeature {
        /// The unsupported feature
        feature: String,
    },

    /// Internal compiler error
    Internal {
        /// Error message
        message: String,
    },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::InvalidAtomIndex { index, max } => {
                write!(f, "Invalid atom index {} (max: {})", index, max)
            }
            CompileError::UnresolvedReference { atom, from } => {
                write!(
                    f,
                    "Unresolved reference to atom {} from instruction {}",
                    atom, from
                )
            }
            CompileError::UnsupportedFeature { feature } => {
                write!(f, "Unsupported feature: {}", feature)
            }
            CompileError::Internal { message } => {
                write!(f, "Internal compiler error: {}", message)
            }
        }
    }
}

impl std::error::Error for CompileError {}

/// Convenience function to compile a grammar
pub fn compile(grammar: Grammar) -> Result<Program, CompileError> {
    Compiler::new(grammar).compile()
}

/// A byte charset for a regex that is a single ASCII-only character class.
///
/// Decoding goes through `CharRanges` (the regex crate's own HIR), so
/// escapes such as `[\x61-\x7a]` mean exactly what the tree-walker's
/// regex path decides. Classes with any non-ASCII member — including
/// negated classes, which resolve to the full Unicode complement — have
/// multi-byte members a one-byte charset cannot consume, so they stay on
/// the regex instruction.
fn ascii_class_set(pattern: &str) -> Option<CharSet> {
    let ascii = *CharRanges::from_pattern(pattern)?.ascii_only()?;
    let mut set = CharSet::new();
    for (b, member) in ascii.into_iter().enumerate() {
        if member {
            set.add(b as u8);
        }
    }
    Some(set)
}

/// The UTF-8 lead bytes of any code point in [lo, hi]. Within one
/// encoding size class the lead byte is monotonic in the code point, so
/// the class-intersection bounds give the exact inclusive lead range —
/// sound by construction (no over- or under-approximation).
fn utf8_first_bytes_range(lo: u32, hi: u32) -> Vec<u8> {
    let lead = |cp: u32| -> u8 {
        if cp < 0x80 {
            cp as u8
        } else if cp < 0x800 {
            (0xC0 | (cp >> 6)) as u8
        } else if cp < 0x10000 {
            (0xE0 | (cp >> 12)) as u8
        } else {
            (0xF0 | (cp >> 18)) as u8
        }
    };
    let mut out = Vec::new();
    if lo < 0x80 {
        for b in (lo as u8)..=(hi.min(0x7F) as u8) {
            out.push(b);
        }
    }
    for (clo, chi) in [(0x80u32, 0x7FFu32), (0x800, 0xFFFF), (0x10000, 0x10FFFF)] {
        let a = lo.max(clo);
        let b = hi.min(chi);
        if a <= b {
            for lead_b in lead(a)..=lead(b) {
                if !out.contains(&lead_b) {
                    out.push(lead_b);
                }
            }
        }
    }
    out
}

/// The exact FIRST-byte union of a regex HIR node, or None when the
/// shape is unprovable (anchors, unanchored repetitions, lookarounds,
/// empty matches). The `hir_first_bytes` method delegates here.
fn hir_first_union(kind: &regex_syntax::hir::HirKind) -> Option<Vec<u8>> {
    use regex_syntax::hir::{Class, HirKind};
    match kind {
        HirKind::Class(Class::Unicode(class)) => {
            let mut union = Vec::new();
            for item in class.iter() {
                for b in utf8_first_bytes_range(item.start() as u32, item.end() as u32) {
                    if !union.contains(&b) {
                        union.push(b);
                    }
                }
            }
            Some(union)
        }
        HirKind::Literal(lit) => {
            let s = std::str::from_utf8(&lit.0).ok()?;
            let first = s.chars().next()?;
            Some(utf8_first_bytes_range(first as u32, first as u32))
        }
        HirKind::Alternation(branches) => {
            let mut union = Vec::new();
            for branch in branches {
                union.extend(hir_first_union(branch.kind())?);
            }
            Some(union)
        }
        // A concat contributes its FIRST child's lead bytes (classes and
        // literals are non-nullable, so later children cannot start the
        // match).
        HirKind::Concat(children) => {
            let first = children.first()?;
            hir_first_union(first.kind())
        }
        HirKind::Capture(cap) => hir_first_union(cap.sub.kind()),
        _ => None,
    }
}

fn push_unique(set: &mut Vec<u8>, b: u8) {
    if !set.contains(&b) {
        set.push(b);
    }
}

/// FIRST evaluation over one atom against the per-rule fixpoint table
/// (rs#574's prover): rule references resolve through the table, so
/// chains and cycles contribute their current partial and the sweep
/// converges to the exact sets.
/// Whole-rule LR-family provability (rs#574 Phase 2, slice A): the
/// per-rule verdict of whether the rule's recognition lowers to a
/// deterministic finite automaton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LrVerdict {
    /// The rule's recognition lowers to a DFA of this many transitions.
    Provable {
        /// The transition count (bytes, branch joins, loop edges).
        states: usize,
    },
    /// The construct that keeps the rule out of the automaton family.
    Refused {
        /// The reason, human-readable.
        reason: &'static str,
    },
}

fn first_of_atom(
    grammar: &Grammar,
    idx: usize,
    table: &HashMap<usize, Option<(Vec<u8>, bool)>>,
    set: &mut Vec<u8>,
    non_null: &mut bool,
) {
    let Some(atom) = grammar.get_atom(idx) else {
        return;
    };
    match atom {
        Atom::Str { pattern } => {
            if let Some(b) = pattern.as_bytes().first() {
                push_unique(set, *b);
                *non_null = true;
            }
        }
        Atom::Re { pattern } => {
            if let Ok(hir) = regex_syntax::parse(pattern) {
                if let Some(bytes) = hir_first_union(hir.kind()) {
                    for b in bytes {
                        push_unique(set, b);
                    }
                    *non_null = true;
                }
            }
        }
        Atom::Sequence { atoms } => {
            for &child in atoms {
                let mut child_nn = false;
                first_of_atom(grammar, child, table, set, &mut child_nn);
                if child_nn {
                    *non_null = true;
                    return;
                }
            }
        }
        Atom::Alternative { atoms } => {
            let mut all_nn = true;
            for &branch in atoms {
                let mut branch_nn = false;
                first_of_atom(grammar, branch, table, set, &mut branch_nn);
                all_nn &= branch_nn;
            }
            *non_null = all_nn;
        }
        Atom::Repetition { atom, min, .. } => {
            let mut body_nn = false;
            first_of_atom(grammar, *atom, table, set, &mut body_nn);
            *non_null = body_nn && *min >= 1;
        }
        Atom::Ignore { atom }
        | Atom::Trivia { atom }
        | Atom::TriviaCapture { atom, .. }
        | Atom::Capture { atom, .. }
        | Atom::Scope { atom } => {
            first_of_atom(grammar, *atom, table, set, non_null);
        }
        Atom::Named { atom, .. } | Atom::Entity { atom } => {
            if let Some(Some((rule_set, rule_nn))) = table.get(atom) {
                for b in rule_set {
                    push_unique(set, *b);
                }
                if *rule_nn {
                    *non_null = true;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::grammar::Atom;
    use crate::portable::grammar::RepetitionTag;

    fn make_simple_grammar() -> Grammar {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "hello".to_string(),
        });
        grammar.root = 0;
        grammar
    }

    /// The fixpoint prover (rs#574): FIRST sets resolve exactly through
    /// call chains AND cycles — the inline walk gave `None` the moment
    /// an Entity re-entered a visited atom.
    #[test]
    fn rule_first_fixpoint_resolves_cycles_exactly() {
        // ruleA = "o" Entity(ruleB)   -> FIRST = {o}
        // ruleB = Entity(ruleA) | "z" -> FIRST = {o, z} via the cycle
        // The old inline walk: Ent(A) -> A -> Ent(B) -> B -> Ent(A)
        // collides with `visited` and yields None.
        let mut grammar = Grammar::new();
        // Reserve the two rule slots first; patch them below.
        let slot_a = grammar.add_atom(Atom::Sequence { atoms: vec![] });
        let slot_b = grammar.add_atom(Atom::Alternative { atoms: vec![] });
        let s_o = grammar.add_atom(Atom::Str {
            pattern: "o".to_string(),
        });
        let s_z = grammar.add_atom(Atom::Str {
            pattern: "z".to_string(),
        });
        let ent_a = grammar.add_atom(Atom::Entity { atom: slot_a });
        let ent_b = grammar.add_atom(Atom::Entity { atom: slot_b });
        grammar.atoms[slot_a] = Atom::Sequence {
            atoms: vec![s_o, ent_b],
        };
        grammar.atoms[slot_b] = Atom::Alternative {
            atoms: vec![ent_a, s_z],
        };
        grammar.root = slot_b;

        let compiler = Compiler::new(grammar);
        let sets = compiler.rule_first_sets();
        let b_first = sets.get(&slot_b).expect("ruleB proved");
        let (set, non_null) = b_first.as_ref().expect("ruleB not poisoned");
        assert!(
            set.contains(&b'o') && set.contains(&b'z'),
            "the cycle contributes through the fixpoint: {set:?}"
        );
        assert!(*non_null, "ruleB matches non-empty");
        let a_first = sets.get(&slot_a).expect("ruleA proved");
        let (aset, _) = a_first.as_ref().expect("ruleA not poisoned");
        assert_eq!(aset, b"o", "ruleA's own first stays exact: {aset:?}");
    }

    /// Build a Named rule atom from children (the boundary the prover
    /// keys on).
    fn named_rule(grammar: &mut Grammar, name: &str, body: usize) -> usize {
        grammar.add_atom(Atom::Named {
            name: name.to_string(),
            atom: body,
        })
    }

    #[test]
    fn lr_provable_rule_proves() {
        // member = '"' chars '"' — a byte chain, the canonical
        // automaton-lowerable shape.
        let mut grammar = Grammar::new();
        let s_q = grammar.add_atom(Atom::Str {
            pattern: "\"".to_string(),
        });
        let s_chars = grammar.add_atom(Atom::Str {
            pattern: "abc".to_string(),
        });
        let body = grammar.add_atom(Atom::Sequence {
            atoms: vec![s_q, s_chars, s_q],
        });
        let rule = named_rule(&mut grammar, "member", body);
        grammar.root = rule;

        let compiler = Compiler::new(grammar);
        let verdicts = compiler.rule_lr_provability();
        match &verdicts[&rule] {
            LrVerdict::Provable { states } => {
                assert!(*states >= 5, "one transition per byte plus joins: {states}");
            }
            LrVerdict::Refused { reason } => {
                panic!("the byte chain must prove: {reason}");
            }
        }
    }

    #[test]
    fn lr_refuses_disjoint_choice_proves() {
        // value = number | string | kw — disjoint FIRST sets prove.
        let mut grammar = Grammar::new();
        let s_num = grammar.add_atom(Atom::Str {
            pattern: "1".to_string(),
        });
        let s_str = grammar.add_atom(Atom::Str {
            pattern: "\"".to_string(),
        });
        let s_kw = grammar.add_atom(Atom::Str {
            pattern: "true".to_string(),
        });
        let body = grammar.add_atom(Atom::Alternative {
            atoms: vec![s_num, s_str, s_kw],
        });
        let rule = named_rule(&mut grammar, "value", body);
        grammar.root = rule;

        let compiler = Compiler::new(grammar);
        let verdicts = compiler.rule_lr_provability();
        assert!(
            matches!(verdicts[&rule], LrVerdict::Provable { .. }),
            "disjoint alternatives prove: {:?}",
            verdicts[&rule]
        );
    }

    #[test]
    fn lr_refuses_overlapping_choice() {
        // "a" | "ab" — the shared lead byte is a DFA conflict.
        let mut grammar = Grammar::new();
        let s_a = grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        let s_ab = grammar.add_atom(Atom::Str {
            pattern: "ab".to_string(),
        });
        let body = grammar.add_atom(Atom::Alternative {
            atoms: vec![s_a, s_ab],
        });
        let rule = named_rule(&mut grammar, "amb", body);
        grammar.root = rule;

        let compiler = Compiler::new(grammar);
        let verdicts = compiler.rule_lr_provability();
        match &verdicts[&rule] {
            LrVerdict::Refused { reason } => {
                assert!(reason.contains("overlapping FIRST"), "{reason}");
            }
            LrVerdict::Provable { states } => {
                panic!("the overlapping choice must refuse, proved {states} states");
            }
        }
    }

    #[test]
    fn lr_refuses_loop_exit_ambiguity() {
        // ("a")* "a" — the iteration and the exit share their lead byte.
        let mut grammar = Grammar::new();
        let s_a = grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        let rep = grammar.add_atom(Atom::Repetition {
            atom: s_a,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        let body = grammar.add_atom(Atom::Sequence {
            atoms: vec![rep, s_a],
        });
        let rule = named_rule(&mut grammar, "run", body);
        grammar.root = rule;

        let compiler = Compiler::new(grammar);
        let verdicts = compiler.rule_lr_provability();
        match &verdicts[&rule] {
            LrVerdict::Refused { reason } => {
                assert!(reason.contains("overlaps what follows"), "{reason}");
            }
            LrVerdict::Provable { states } => {
                panic!("the ambiguous loop must refuse, proved {states} states");
            }
        }
    }

    #[test]
    fn lr_refuses_recursion() {
        // rule = "x" rule? — the closure is not finite.
        let mut grammar = Grammar::new();
        let slot = grammar.add_atom(Atom::Sequence { atoms: vec![] });
        let s_x = grammar.add_atom(Atom::Str {
            pattern: "x".to_string(),
        });
        let ent = grammar.add_atom(Atom::Entity { atom: slot });
        let rep = grammar.add_atom(Atom::Repetition {
            atom: ent,
            min: 0,
            max: Some(1),
            tag: RepetitionTag::Maybe,
        });
        grammar.atoms[slot] = Atom::Sequence {
            atoms: vec![s_x, rep],
        };
        let rule = named_rule(&mut grammar, "rec", slot);
        grammar.root = rule;

        let compiler = Compiler::new(grammar);
        let verdicts = compiler.rule_lr_provability();
        match &verdicts[&rule] {
            LrVerdict::Refused { reason } => {
                assert!(reason.contains("recursion"), "{reason}");
            }
            LrVerdict::Provable { states } => {
                panic!("the recursive rule must refuse, proved {states} states");
            }
        }
    }

    #[test]
    fn lr_refuses_predicates() {
        let mut grammar = Grammar::new();
        let s_a = grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        let la = grammar.add_atom(Atom::Lookahead {
            atom: s_a,
            positive: true,
        });
        let body = grammar.add_atom(Atom::Sequence {
            atoms: vec![la, s_a],
        });
        let rule = named_rule(&mut grammar, "pred", body);
        grammar.root = rule;

        let compiler = Compiler::new(grammar);
        let verdicts = compiler.rule_lr_provability();
        assert!(
            matches!(&verdicts[&rule], LrVerdict::Refused { .. }),
            "a lookahead refuses: {:?}",
            verdicts[&rule]
        );
    }

    /// The fleet survey: per-flavor Provable/Refused rule counts over
    /// the sibling pubid-grammar artifacts — the coverage evidence the
    /// automaton-routing slice sizes against. Ambient stale checkouts
    /// skip with guidance (the parg_corpus philosophy).
    #[test]
    fn lr_survey_over_the_corpus_fleet() {
        let dir = match std::env::var("PARG_ARTIFACT_DIR") {
            Ok(dir) => std::path::PathBuf::from(dir),
            Err(_) => {
                let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                match manifest
                    .ancestors()
                    .map(|a| a.join("pubid").join("pubid-grammar").join("artifacts"))
                    .find(|c| c.is_dir())
                {
                    Some(dir) => dir,
                    None => {
                        eprintln!(
                            "skip: no PARG_ARTIFACT_DIR and no sibling pubid-grammar checkout"
                        );
                        return;
                    }
                }
            }
        };
        let mut flavors = 0usize;
        let mut provable_total = 0usize;
        let mut refused_total = 0usize;
        let mut reasons: std::collections::BTreeMap<&'static str, usize> = Default::default();
        for entry in std::fs::read_dir(&dir).expect("artifact dir") {
            let path = entry.expect("entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let Some(grammar_value) = value["entries"]
                .as_object()
                .and_then(|e| e.values().next())
                .map(|e| &e["grammar"])
            else {
                continue;
            };
            let Ok(grammar) = serde_json::from_value::<Grammar>(grammar_value.clone()) else {
                continue;
            };
            let compiler = Compiler::new(grammar);
            flavors += 1;
            for verdict in compiler.rule_lr_provability().values() {
                match verdict {
                    LrVerdict::Provable { .. } => provable_total += 1,
                    LrVerdict::Refused { reason } => {
                        refused_total += 1;
                        *reasons.entry(*reason).or_insert(0) += 1;
                    }
                }
            }
        }
        eprintln!(
            "LR survey: {flavors} flavors, {provable_total} provable rules, {refused_total} refused",
        );
        for (reason, count) in &reasons {
            eprintln!("  {count:5}  {reason}");
        }
        assert!(flavors >= 20, "the survey saw only {flavors} flavors");
    }

    #[test]
    fn test_compile_string() {
        let grammar = make_simple_grammar();
        let program = Compiler::new(grammar).compile().unwrap();

        assert!(program.instruction_count() > 0);
        assert!(program.string_count() > 0);
    }

    #[test]
    fn test_compile_char() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        grammar.root = 0;

        let program = Compiler::new(grammar).compile().unwrap();

        // Single char should use Char instruction
        let instr = program.get_instruction(0).unwrap();
        assert!(matches!(instr, Instruction::Char { .. }));
    }

    #[test]
    fn test_compile_sequence() {
        let mut grammar = Grammar::new();
        let a = grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        let b = grammar.add_atom(Atom::Str {
            pattern: "b".to_string(),
        });
        grammar.add_atom(Atom::Sequence { atoms: vec![a, b] });
        grammar.root = 2;

        let program = Compiler::new(grammar).compile().unwrap();

        // Should have Char 'a', Char 'b', End
        assert!(program.instruction_count() >= 2);
    }

    #[test]
    fn test_compile_alternative() {
        let mut grammar = Grammar::new();
        let a = grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        let b = grammar.add_atom(Atom::Str {
            pattern: "b".to_string(),
        });
        grammar.add_atom(Atom::Alternative { atoms: vec![a, b] });
        grammar.root = 2;

        let program = Compiler::new(grammar).compile().unwrap();

        // Should have Choice, Char 'a', Jump, Char 'b', End
        assert!(program.instruction_count() >= 3);
    }

    #[test]
    fn test_compile_repetition() {
        let mut grammar = Grammar::new();
        let a = grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        grammar.add_atom(Atom::Repetition {
            atom: a,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        grammar.root = 1;

        let program = Compiler::new(grammar).compile().unwrap();

        // Should have loop structure
        assert!(program.instruction_count() >= 2);
    }

    #[test]
    fn test_compile_regex() {
        let mut grammar = Grammar::new();
        grammar.add_atom(Atom::Re {
            pattern: "[0-9]".to_string(),
        });
        grammar.root = 0;

        let program = Compiler::new(grammar).compile().unwrap();

        // Should use charset optimization for simple character class
        assert!(program.char_set_count() > 0);
    }

    #[test]
    fn test_compile_named() {
        let mut grammar = Grammar::new();
        let a = grammar.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        grammar.add_atom(Atom::Named {
            name: "letter".to_string(),
            atom: a,
        });
        grammar.root = 1;

        let program = Compiler::new(grammar).compile().unwrap();

        // Named compiles to the body followed by a BuildHash envelope,
        // mirroring the tree-walker's {name: value} wrapping.
        let has_build_hash = program
            .instructions()
            .iter()
            .any(|i| matches!(i, Instruction::BuildHash { .. }));

        assert!(has_build_hash);
    }

    #[test]
    fn test_program_disassembly() {
        let grammar = make_simple_grammar();
        let program = Compiler::new(grammar).compile().unwrap();

        let disasm = program.disassemble();
        assert!(disasm.contains("String"));
        assert!(disasm.contains("End"));
    }
}
