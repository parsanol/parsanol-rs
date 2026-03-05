//! Compiler from Grammar to bytecode Program
//!
//! This module implements the compiler that transforms parsanol's `Grammar`
//! (represented as `Atom` nodes) into a `Program` of bytecode instructions.

use super::instruction::{CaptureKind, Instruction};
use super::program::{CharSet, Program};
use crate::portable::char_class::CharacterPattern;
use crate::portable::grammar::{Atom, Grammar};

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
        }
    }

    /// Compile the grammar into a program
    pub fn compile(mut self) -> Result<Program, CompileError> {
        // Compile the root atom
        let entry = self.compile_atom(self.grammar.root)?;

        // Set entry point
        self.program.set_entry_point(entry);

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
        let atom = self.grammar.get_atom(atom_idx).cloned().ok_or(
            CompileError::InvalidAtomIndex {
                index: atom_idx,
                max: self.grammar.atoms.len(),
            }
        )?;

        // Only cache addresses for Entity atoms (rule references)
        // Inline atoms (Str, Re, etc.) should be compiled fresh each time
        if matches!(atom, Atom::Entity { .. }) {
            // Check if this rule already has a compiled address
            if let Some(addr) = self.program.get_rule_address(atom_idx) {
                return Ok(addr);
            }
        }

        // Record the entry point for this atom (primarily for Entity references)
        let entry = self.program.instruction_count();
        self.program.add_rule_address(atom_idx, entry);

        self.state.current_atom = atom_idx;

        match atom {
            Atom::Str { pattern } => self.compile_str(&pattern),
            Atom::Re { pattern } => self.compile_re(&pattern),
            Atom::Sequence { atoms } => self.compile_sequence(&atoms),
            Atom::Alternative { atoms } => self.compile_alternative(&atoms),
            Atom::Repetition { atom, min, max } => self.compile_repetition(atom, min, max),
            Atom::Named { name, atom } => self.compile_named(&name, atom),
            Atom::Entity { atom } => self.compile_entity(atom),
            Atom::Lookahead { atom, positive } => self.compile_lookahead(atom, positive),
            Atom::Cut => self.compile_cut(),
            Atom::Ignore { atom } => self.compile_ignore(atom),
            Atom::Custom { id } => self.compile_custom(id),
        }
    }

    /// Compile a string literal
    fn compile_str(&mut self, pattern: &str) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        if pattern.is_empty() {
            // Empty string always matches
            return Ok(entry);
        }

        if pattern.len() == 1 {
            // Single character: use Char instruction
            self.program.add_instruction(Instruction::char(pattern.as_bytes()[0]));
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

        if atoms.is_empty() {
            return Ok(entry);
        }

        // Compile each atom in sequence
        for &atom_idx in atoms {
            self.compile_atom(atom_idx)?;
        }

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

        // For alternatives (ordered choice in PEG):
        // Choice L2        ; if alt1 fails, backtrack to L2
        // <alt1>
        // Jump End         ; alt1 succeeded, skip remaining alts
        // L2: Choice L3    ; if alt2 fails, backtrack to L3
        // <alt2>
        // Jump End
        // ...
        // Ln: <altN>       ; last alternative, no choice needed
        // End:

        // First, add all Choice instructions with placeholder offsets
        // We need (n-1) choices for n alternatives
        let choice_indices: Vec<usize> = (0..atoms.len() - 1)
            .map(|_| {
                let idx = self.program.instruction_count();
                self.program.add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));
                idx
            })
            .collect();

        // Compile each alternative, tracking where each starts
        let mut alt_starts = Vec::with_capacity(atoms.len());
        let mut jump_indices = Vec::with_capacity(atoms.len() - 1);

        for (i, &atom_idx) in atoms.iter().enumerate() {
            // Record where this alternative starts
            alt_starts.push(self.program.instruction_count());

            // Compile the alternative
            self.compile_atom(atom_idx)?;

            // If not the last alternative, add Jump to end
            if i < atoms.len() - 1 {
                let jump_idx = self.program.instruction_count();
                self.program.add_instruction(Instruction::jump(PLACEHOLDER_OFFSET));
                jump_indices.push(jump_idx);
            }
        }

        let end_idx = self.program.instruction_count();

        // Patch Choice instructions: each Choice should backtrack to the next alternative
        for (i, &choice_idx) in choice_indices.iter().enumerate() {
            // Choice i should jump to alternative (i+1)
            let next_alt_start = alt_starts[i + 1];
            let offset = (next_alt_start as i32) - (choice_idx as i32 + 1);
            self.program.set_instruction(choice_idx, Instruction::choice(offset));
        }

        // Patch Jump instructions: each Jump should skip to the end
        for &jump_idx in &jump_indices {
            let offset = (end_idx as i32) - (jump_idx as i32 + 1);
            self.program.set_instruction(jump_idx, Instruction::jump(offset));
        }

        Ok(entry)
    }

    /// Compile repetition (min, max)
    fn compile_repetition(
        &mut self,
        atom_idx: usize,
        min: usize,
        max: Option<usize>,
    ) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        if min == 0 && max == Some(0) {
            // Zero repetitions: always matches
            return Ok(entry);
        }

        if min == 0 && max == Some(1) {
            // Optional: Choice, <atom>, Commit
            let choice_idx = self.program.instruction_count();
            self.program.add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));
            self.compile_atom(atom_idx)?;
            let _commit_idx = self.program.instruction_count();
            self.program.add_instruction(Instruction::commit(0));
            let after_commit = self.program.instruction_count();

            // Patch choice to skip to after commit on failure
            let choice_offset = (after_commit as i32) - (choice_idx as i32 + 1);
            self.program.set_instruction(choice_idx, Instruction::choice(choice_offset));

            return Ok(entry);
        }

        // For min..max:
        // First, match 'min' times (mandatory)
        for _ in 0..min {
            self.compile_atom(atom_idx)?;
        }

        // Then, optionally match up to (max - min) more times
        match max {
            None => {
                // Unlimited: loop until failure
                // Use Span optimization if possible for character classes
                if let Some(set_idx) = self.try_get_charset_for_atom(atom_idx) {
                    // Optimize: use Span instruction
                    self.program.add_instruction(Instruction::span(set_idx));
                } else {
                    // General case: loop with Choice/PartialCommit
                    // Structure:
                    //   loop_start: Choice partial_commit (skip to after loop on failure)
                    //   <atom>
                    //   PartialCommit loop_start
                    //   after_loop:
                    let loop_start = self.program.instruction_count();
                    self.program.add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));
                    self.compile_atom(atom_idx)?;
                    let partial_commit_idx = self.program.instruction_count();

                    // Calculate offset to loop back
                    let loop_offset = (loop_start as i32) - (partial_commit_idx as i32 + 1);
                    self.program.add_instruction(Instruction::partial_commit(loop_offset));

                    let after_loop = self.program.instruction_count();

                    // Patch choice to skip to after loop on failure
                    let choice_offset = (after_loop as i32) - (loop_start as i32 + 1);
                    self.program.set_instruction(loop_start, Instruction::choice(choice_offset));
                }
            }
            Some(max_val) => {
                // Limited: try to match up to (max - min) more times
                for _ in 0..(max_val - min) {
                    let choice_idx = self.program.instruction_count();
                    self.program.add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));
                    self.compile_atom(atom_idx)?;
                    let after_atom = self.program.instruction_count();

                    // Patch choice to skip to after atom on failure
                    let choice_offset = (after_atom as i32) - (choice_idx as i32 + 1);
                    self.program.set_instruction(choice_idx, Instruction::choice(choice_offset));
                }
            }
        }

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

        let key_idx = self.program.add_key(name);

        // OpenCapture, <atom>, CloseCapture
        self.program
            .add_instruction(Instruction::open_capture(CaptureKind::Named, key_idx));
        self.compile_atom(atom_idx)?;
        self.program
            .add_instruction(Instruction::close_capture(CaptureKind::Named, key_idx));

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
            // Forward reference: use placeholder, patch later
            self.program.add_instruction(Instruction::call(PLACEHOLDER_OFFSET));
            self.pending_patches.push((entry, atom_idx));
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
            self.program.add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));

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
            self.program.add_instruction(Instruction::choice(PLACEHOLDER_OFFSET));

            self.compile_atom(atom_idx)?;
            self.program.add_instruction(Instruction::fail_twice());

            let success_idx = self.program.instruction_count();

            // Patch choice to jump to success
            let choice_offset = (success_idx as i32) - (choice_idx as i32 + 1);
            self.program
                .set_instruction(choice_idx, Instruction::choice(choice_offset));
        }

        Ok(entry)
    }

    /// Compile cut (atomic predicate)
    fn compile_cut(&mut self) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // Cut: commit the current choice, preventing backtracking
        // This is implemented as FailTwice which pops the choice and fails
        // But in the context of the grammar, Cut should never be reached on success
        // It's used to prevent backtracking in sequences

        // Actually, Cut in PEG means "commit to this alternative"
        // We implement it by committing all pending choices
        self.program.add_instruction(Instruction::commit(0));

        Ok(entry)
    }

    /// Compile ignore (match but discard result)
    fn compile_ignore(&mut self, atom_idx: usize) -> Result<usize, CompileError> {
        let entry = self.program.instruction_count();

        // Compile the atom, then push a Nil result
        self.compile_atom(atom_idx)?;

        // For now, we don't track results at the VM level
        // The capture system handles result building
        // Ignore just means don't create a capture

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
                }
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
                write!(f, "Unresolved reference to atom {} from instruction {}", atom, from)
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

        // Should have OpenCapture, Char, CloseCapture
        let has_open = program
            .instructions()
            .iter()
            .any(|i| matches!(i, Instruction::OpenCapture { .. }));
        let has_close = program
            .instructions()
            .iter()
            .any(|i| matches!(i, Instruction::CloseCapture { .. }));

        assert!(has_open);
        assert!(has_close);
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
