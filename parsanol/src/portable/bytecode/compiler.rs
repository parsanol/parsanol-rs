//! Compiler from Grammar to bytecode Program
//!
//! This module implements the compiler that transforms parsanol's `Grammar`
//! (represented as `Atom` nodes) into a `Program` of bytecode instructions.

use super::instruction::Instruction;
use super::program::{CharSet, Program};
use crate::portable::char_class::CharacterPattern;
use crate::portable::grammar::{Atom, Grammar, RepetitionTag};
use std::collections::VecDeque;

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
        Self {
            grammar,
            program: Program::with_capacity(atom_count * 4, atom_count, atom_count / 4),
            state: CompilerState { current_atom: 0 },
            pending_patches: Vec::new(),
            subroutine_queue: VecDeque::new(),
            compile_depth: 0,
            rule_atoms,
            compiled_rules: std::collections::HashSet::new(),
            compiling_rule_body: false,
            emitted_dispatch: false,
        }
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
            } => self.compile_repetition(atom, min, max, tag),
            Atom::Named { name, atom } => self.compile_named(&name, atom),
            Atom::Entity { atom } => self.compile_reference(atom),
            Atom::Lookahead { atom, positive } => self.compile_lookahead(atom, positive),
            Atom::Cut => self.compile_cut(),
            Atom::Ignore { atom } => self.compile_ignore(atom),
            Atom::Capture { name, atom } => self.compile_capture(&name, atom),
            Atom::Scope { atom } => self.compile_scope(atom),
            Atom::Dynamic { callback_id } => self.compile_dynamic(callback_id),
            Atom::Custom { id } => self.compile_custom(id),
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

        // Check for simple character ranges
        if pattern.starts_with('[') && pattern.ends_with(']') {
            if let Some(set) = self.parse_char_class(pattern) {
                let set_idx = self.program.add_char_set(set);
                self.program.add_instruction(Instruction::charset(set_idx));
                return Ok(entry);
            }
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

    /// Try to parse a character class like [a-z], [abc], or [^...]
    fn parse_char_class(&self, pattern: &str) -> Option<CharSet> {
        let inner = &pattern[1..pattern.len() - 1]; // Remove brackets

        // Check for negation
        let (chars, negated) = if let Some(stripped) = inner.strip_prefix('^') {
            (stripped, true)
        } else {
            (inner, false)
        };

        let mut set = CharSet::new();

        let mut i = 0;
        let bytes = chars.as_bytes();

        while i < bytes.len() {
            if i + 2 < bytes.len() && bytes[i + 1] == b'-' {
                // Range: a-z
                let start = bytes[i];
                let end = bytes[i + 2];
                for b in start..=end {
                    set.add(b);
                }
                i += 3;
            } else {
                set.add(bytes[i]);
                i += 1;
            }
        }

        // Negate if needed
        if negated {
            set.negate();
        }

        Some(set)
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
            Atom::Sequence { atoms } => {
                // Union of leading nullable children's sets, then the
                // first provably non-nullable child's set. A nullable
                // child CAN consume its lead byte, so its set must be
                // part of the union for ordering to stay sound.
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
            Atom::Named { atom, .. }
            | Atom::Entity { atom }
            | Atom::Ignore { atom }
            | Atom::Capture { atom, .. }
            | Atom::Scope { atom } => self.provable_first_set(*atom, visited),
            Atom::Lookahead { .. } | Atom::Cut | Atom::Dynamic { .. } | Atom::Custom { .. } => None,
        };
        visited.remove(&atom_idx);
        result
    }

    /// Lead bytes of a regex we can prove: a single character class
    /// `[...]` with no quantifier, alternation, or anchor anything else.
    fn regex_first_bytes(&self, pattern: &str) -> Option<Vec<u8>> {
        if !(pattern.starts_with('[') && pattern.ends_with(']')) || pattern.len() < 3 {
            return None;
        }
        let inner = &pattern[1..pattern.len() - 1];
        if inner.contains("||") || inner.is_empty() {
            return None;
        }
        let mut negated = false;
        let mut chars = inner.char_indices().peekable();
        if let Some((_, first)) = chars.peek() {
            if *first == '^' {
                negated = true;
                chars.next();
            }
        }
        let mut allowed = [false; 256];
        let mut any = false;
        let bytes = inner.as_bytes();
        let _ = chars;
        let mut i = if negated { 1 } else { 0 };
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    let esc = bytes.get(i + 1)?;
                    match esc {
                        b'd' => (b'0'..=b'9').for_each(|b| allowed[b as usize] = true),
                        b'w' => {
                            (b'a'..=b'z').for_each(|b| allowed[b as usize] = true);
                            (b'A'..=b'Z').for_each(|b| allowed[b as usize] = true);
                            (b'0'..=b'9').for_each(|b| allowed[b as usize] = true);
                            allowed[b'_' as usize] = true;
                        }
                        b's' | b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c => {
                            // \s and literal whitespace escapes
                            for b in [b' ', b'\t', b'\n', b'\r', 0x0b, 0x0c] {
                                allowed[b as usize] = true;
                            }
                        }
                        b'n' => allowed[b'\n' as usize] = true,
                        b'r' => allowed[b'\r' as usize] = true,
                        b't' => allowed[b'\t' as usize] = true,
                        b'f' => allowed[0x0c] = true,
                        b'v' => allowed[0x0b] = true,
                        other => allowed[*other as usize] = true,
                    }
                    any = true;
                    i += 2;
                }
                b'-' if i > 0 && i + 1 < bytes.len() => {
                    let lo = bytes[i - 1];
                    let hi = bytes[i + 1];
                    if lo > hi {
                        return None;
                    }
                    (lo..=hi).for_each(|b| allowed[b as usize] = true);
                    any = true;
                    i += 2;
                }
                b']' if i > 0 => {
                    return None; // nested class: unsupported
                }
                b => {
                    allowed[b as usize] = true;
                    any = true;
                    i += 1;
                }
            }
        }
        if negated {
            for slot in allowed.iter_mut() {
                *slot = !*slot;
            }
            any = true;
        }
        if !any {
            return None;
        }
        Some(
            (0..256u32)
                .filter(|&b| allowed[b as usize])
                .map(|b| b as u8)
                .collect(),
        )
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

        // The tree-walker collects one value per iteration into a tagged
        // envelope; a present Maybe flattens to its single value.
        self.program.add_instruction(Instruction::rep_open());

        if min == 0 && max == Some(0) {
            self.program.add_instruction(Instruction::build_rep(maybe));
            return Ok(entry);
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

    /// Try to get the charset index for an atom if it's a simple charset
    fn try_get_charset_for_atom(&mut self, atom_idx: usize) -> Option<u32> {
        let atom = self.grammar.get_atom(atom_idx)?;

        match atom {
            Atom::Re { pattern } => {
                if let Some(char_pattern) = CharacterPattern::from_pattern(pattern) {
                    let set = self.char_pattern_to_set(char_pattern);
                    Some(self.program.add_char_set(set))
                } else if pattern.starts_with('[') && pattern.ends_with(']') {
                    let set = self.parse_char_class(pattern)?;
                    Some(self.program.add_char_set(set))
                } else {
                    None
                }
            }
            Atom::Str { pattern } if pattern.len() == 1 => {
                let mut set = CharSet::new();
                set.add(pattern.as_bytes()[0]);
                Some(self.program.add_char_set(set))
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
