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
}

impl Compiler {
    /// Create a new compiler for the given grammar
    #[inline]
    pub fn new(grammar: Grammar) -> Self {
        let atom_count = grammar.atoms.len();
        Self {
            grammar,
            program: Program::with_capacity(atom_count * 4, atom_count, atom_count / 4),
            state: CompilerState { current_atom: 0 },
            pending_patches: Vec::new(),
            subroutine_queue: VecDeque::new(),
        }
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
            if self.program.get_rule_address(atom_idx).is_some() {
                continue;
            }
            self.compile_atom(atom_idx)?;
            self.program.add_instruction(Instruction::ret());
        }

        // Add final End instruction
        self.program.add_instruction(Instruction::end());

        // Patch all forward references
        self.patch_references()?;

        // Optimize the program
        self.program.optimize();

        Ok(self.program)
    }

    /// Compile a single atom and return the entry instruction index
    fn compile_atom(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
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
            Atom::Entity { atom } => self.compile_entity(atom),
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

    /// Compile alternatives (ordered choice)
    fn compile_alternative(&mut self, atoms: &[usize]) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        if atoms.is_empty() {
            return Ok(entry);
        }

        if atoms.len() == 1 {
            return self.compile_atom(atoms[0]);
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

    /// Compile an entity reference (forward reference)
    fn compile_entity(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // Check if the target atom is already compiled
        if let Some(target_addr) = self.program.get_rule_address(atom_idx) {
            // Direct call
            let offset = (target_addr as i32) - (entry as i32 + 1);
            self.program.add_instruction(Instruction::call(offset));
        } else {
            // Forward reference: use placeholder, patch later. The body is
            // queued as a trailing subroutine — it must not be compiled
            // inline after the call, because the call's return address is
            // the instruction that follows it.
            self.program
                .add_instruction(Instruction::call(PLACEHOLDER_OFFSET));
            self.pending_patches.push((entry, atom_idx));
            self.subroutine_queue.push_back(atom_idx);
        }

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
    fn compile_dynamic(&mut self, _callback_id: u64) -> Result<usize, CompileError> {
        Err(CompileError::UnsupportedFeature {
            feature: "Dynamic atoms (phase 3; grammars using them stay on the packrat engine)"
                .to_string(),
        })
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
