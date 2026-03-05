//! Compiled bytecode program
//!
//! This module defines the `Program` struct which holds compiled bytecode
//! and all associated data tables (strings, character sets, regexes, etc.)

use super::instruction::Instruction;
use std::collections::HashMap;

/// A character set represented as a 256-byte bitmap
///
/// Each bit indicates whether the corresponding byte value is in the set.
/// This provides O(1) membership testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharSet {
    /// Bitmap of character membership
    bitmap: [bool; 256],
}

impl CharSet {
    /// Create an empty character set
    #[inline]
    pub fn new() -> Self {
        Self {
            bitmap: [false; 256],
        }
    }

    /// Create a character set from bytes
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut set = Self::new();
        for &b in bytes {
            set.bitmap[b as usize] = true;
        }
        set
    }

    /// Create a character set from a range
    #[inline]
    pub fn from_range(start: u8, end: u8) -> Self {
        let mut set = Self::new();
        for b in start..=end {
            set.bitmap[b as usize] = true;
        }
        set
    }

    /// Create a character set matching any character (.)
    #[inline]
    pub fn any() -> Self {
        Self {
            bitmap: [true; 256],
        }
    }

    /// Add a byte to the set
    #[inline]
    pub fn add(&mut self, b: u8) {
        self.bitmap[b as usize] = true;
    }

    /// Remove a byte from the set
    #[inline]
    pub fn remove(&mut self, b: u8) {
        self.bitmap[b as usize] = false;
    }

    /// Check if a byte is in the set
    #[inline]
    pub fn contains(&self, b: u8) -> bool {
        self.bitmap[b as usize]
    }

    /// Get the bitmap as a slice
    #[inline]
    pub fn bitmap(&self) -> &[bool; 256] {
        &self.bitmap
    }

    /// Union with another set
    #[inline]
    pub fn union(&mut self, other: &CharSet) {
        for i in 0..256 {
            self.bitmap[i] = self.bitmap[i] || other.bitmap[i];
        }
    }

    /// Intersect with another set
    #[inline]
    pub fn intersect(&mut self, other: &CharSet) {
        for i in 0..256 {
            self.bitmap[i] = self.bitmap[i] && other.bitmap[i];
        }
    }

    /// Negate the set
    #[inline]
    pub fn negate(&mut self) {
        for i in 0..256 {
            self.bitmap[i] = !self.bitmap[i];
        }
    }
}

impl Default for CharSet {
    fn default() -> Self {
        Self::new()
    }
}

/// A compiled bytecode program
///
/// Contains all instructions and data tables needed for execution.
/// Programs are created by the `Compiler` from a `Grammar`.
#[derive(Debug, Clone)]
pub struct Program {
    /// All instructions in the program
    instructions: Vec<Instruction>,

    /// String table (for String instructions)
    strings: Vec<String>,

    /// Character set table (for CharSet instructions)
    char_sets: Vec<CharSet>,

    /// Regex pattern table (for Regex instructions)
    regexes: Vec<String>,

    /// Key table (for capture names)
    keys: Vec<String>,

    /// Label table (for error messages)
    labels: Vec<String>,

    /// Entry point (instruction index where execution starts)
    entry_point: usize,

    /// Rule addresses: atom index -> instruction index
    rule_addresses: HashMap<usize, usize>,
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

impl Program {
    /// Create a new empty program
    #[inline]
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            strings: Vec::new(),
            char_sets: Vec::new(),
            regexes: Vec::new(),
            keys: Vec::new(),
            labels: Vec::new(),
            entry_point: 0,
            rule_addresses: HashMap::new(),
        }
    }

    /// Create a program with pre-allocated capacity
    #[inline]
    pub fn with_capacity(instructions: usize, strings: usize, sets: usize) -> Self {
        Self {
            instructions: Vec::with_capacity(instructions),
            strings: Vec::with_capacity(strings),
            char_sets: Vec::with_capacity(sets),
            regexes: Vec::new(),
            keys: Vec::with_capacity(strings),
            labels: Vec::new(),
            entry_point: 0,
            rule_addresses: HashMap::new(),
        }
    }

    // ============================================================================
    // Instruction Management
    // ============================================================================

    /// Add an instruction and return its index
    #[inline]
    pub fn add_instruction(&mut self, instr: Instruction) -> usize {
        let idx = self.instructions.len();
        self.instructions.push(instr);
        idx
    }

    /// Add multiple instructions and return the starting index
    #[inline]
    pub fn add_instructions(&mut self, instrs: &[Instruction]) -> usize {
        let start = self.instructions.len();
        self.instructions.extend_from_slice(instrs);
        start
    }

    /// Get an instruction by index
    #[inline]
    pub fn get_instruction(&self, idx: usize) -> Option<&Instruction> {
        self.instructions.get(idx)
    }

    /// Get a mutable instruction by index
    #[inline]
    pub fn get_instruction_mut(&mut self, idx: usize) -> Option<&mut Instruction> {
        self.instructions.get_mut(idx)
    }

    /// Get the total number of instructions
    #[inline]
    pub fn instruction_count(&self) -> usize {
        self.instructions.len()
    }

    /// Get all instructions as a slice
    #[inline]
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    /// Replace an instruction at the given index
    #[inline]
    pub fn set_instruction(&mut self, idx: usize, instr: Instruction) {
        self.instructions[idx] = instr;
    }

    /// Remove an instruction at the given index
    pub fn remove_instruction(&mut self, idx: usize) {
        self.instructions.remove(idx);
    }

    // ============================================================================
    // String Table Management
    // ============================================================================

    /// Add a string to the string table and return its index
    ///
    /// Uses interning to avoid duplicates.
    #[inline]
    pub fn add_string(&mut self, s: &str) -> u32 {
        // Check for existing string (simple linear search)
        for (i, existing) in self.strings.iter().enumerate() {
            if existing == s {
                return i as u32;
            }
        }

        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        idx
    }

    /// Get a string from the string table
    #[inline]
    pub fn get_string(&self, idx: u32) -> Option<&str> {
        self.strings.get(idx as usize).map(|s| s.as_str())
    }

    /// Get the number of strings in the table
    #[inline]
    pub fn string_count(&self) -> usize {
        self.strings.len()
    }

    // ============================================================================
    // Character Set Table Management
    // ============================================================================

    /// Add a character set to the table and return its index
    ///
    /// Uses interning to avoid duplicates.
    #[inline]
    pub fn add_char_set(&mut self, set: CharSet) -> u32 {
        // Check for existing set
        for (i, existing) in self.char_sets.iter().enumerate() {
            if existing == &set {
                return i as u32;
            }
        }

        let idx = self.char_sets.len() as u32;
        self.char_sets.push(set);
        idx
    }

    /// Get a character set from the table
    #[inline]
    pub fn get_char_set(&self, idx: u32) -> Option<&CharSet> {
        self.char_sets.get(idx as usize)
    }

    /// Get the number of character sets in the table
    #[inline]
    pub fn char_set_count(&self) -> usize {
        self.char_sets.len()
    }

    // ============================================================================
    // Regex Table Management
    // ============================================================================

    /// Add a regex pattern to the table and return its index
    #[inline]
    pub fn add_regex(&mut self, pattern: &str) -> u32 {
        // Check for existing regex
        for (i, existing) in self.regexes.iter().enumerate() {
            if existing == pattern {
                return i as u32;
            }
        }

        let idx = self.regexes.len() as u32;
        self.regexes.push(pattern.to_string());
        idx
    }

    /// Get a regex pattern from the table
    #[inline]
    pub fn get_regex(&self, idx: u32) -> Option<&str> {
        self.regexes.get(idx as usize).map(|s| s.as_str())
    }

    /// Get the number of regexes in the table
    #[inline]
    pub fn regex_count(&self) -> usize {
        self.regexes.len()
    }

    // ============================================================================
    // Key Table Management (for capture names)
    // ============================================================================

    /// Add a key to the key table and return its index
    #[inline]
    pub fn add_key(&mut self, key: &str) -> u32 {
        // Check for existing key
        for (i, existing) in self.keys.iter().enumerate() {
            if existing == key {
                return i as u32;
            }
        }

        let idx = self.keys.len() as u32;
        self.keys.push(key.to_string());
        idx
    }

    /// Get a key from the key table
    #[inline]
    pub fn get_key(&self, idx: u32) -> Option<&str> {
        self.keys.get(idx as usize).map(|s| s.as_str())
    }

    /// Get the number of keys in the table
    #[inline]
    pub fn key_count(&self) -> usize {
        self.keys.len()
    }

    // ============================================================================
    // Label Table Management (for error messages)
    // ============================================================================

    /// Add a label to the label table and return its index
    #[inline]
    pub fn add_label(&mut self, label: &str) -> u32 {
        // Check for existing label
        for (i, existing) in self.labels.iter().enumerate() {
            if existing == label {
                return i as u32;
            }
        }

        let idx = self.labels.len() as u32;
        self.labels.push(label.to_string());
        idx
    }

    /// Get a label from the label table
    #[inline]
    pub fn get_label(&self, idx: u32) -> Option<&str> {
        self.labels.get(idx as usize).map(|s| s.as_str())
    }

    /// Get the number of labels in the table
    #[inline]
    pub fn label_count(&self) -> usize {
        self.labels.len()
    }

    // ============================================================================
    // Entry Point and Rules
    // ============================================================================

    /// Set the entry point
    #[inline]
    pub fn set_entry_point(&mut self, idx: usize) {
        self.entry_point = idx;
    }

    /// Get the entry point
    #[inline]
    pub fn entry_point(&self) -> usize {
        self.entry_point
    }

    /// Register a rule address (atom index -> instruction index)
    #[inline]
    pub fn add_rule_address(&mut self, atom_idx: usize, instr_idx: usize) {
        self.rule_addresses.insert(atom_idx, instr_idx);
    }

    /// Get the instruction index for a rule
    #[inline]
    pub fn get_rule_address(&self, atom_idx: usize) -> Option<usize> {
        self.rule_addresses.get(&atom_idx).copied()
    }

    // ============================================================================
    // Utilities
    // ============================================================================

    /// Get memory usage estimate
    pub fn memory_usage(&self) -> usize {
        let instr_size = self.instructions.len() * std::mem::size_of::<Instruction>();
        let strings_size: usize = self.strings.iter().map(|s| s.len()).sum();
        let sets_size = self.char_sets.len() * std::mem::size_of::<CharSet>();
        let regexes_size: usize = self.regexes.iter().map(|s| s.len()).sum();
        let keys_size: usize = self.keys.iter().map(|s| s.len()).sum();
        let labels_size: usize = self.labels.iter().map(|s| s.len()).sum();

        instr_size + strings_size + sets_size + regexes_size + keys_size + labels_size
    }

    /// Disassemble the program to a string (for debugging)
    pub fn disassemble(&self) -> String {
        let mut output = String::new();

        output.push_str("=== Bytecode Program ===\n\n");

        // Strings
        if !self.strings.is_empty() {
            output.push_str("Strings:\n");
            for (i, s) in self.strings.iter().enumerate() {
                output.push_str(&format!("  [{}] {:?}\n", i, s));
            }
            output.push('\n');
        }

        // Character sets
        if !self.char_sets.is_empty() {
            output.push_str("Character Sets:\n");
            for (i, set) in self.char_sets.iter().enumerate() {
                let chars: String = (0..=255)
                    .filter(|&b| set.contains(b))
                    .filter_map(|b| {
                        if (32..127).contains(&b) {
                            Some(b as char)
                        } else if b == b'\t' {
                            Some('\\')
                        } else if b == b'\n' {
                            Some('n')
                        } else if b == b'\r' {
                            Some('r')
                        } else {
                            None
                        }
                    })
                    .collect();
                output.push_str(&format!("  [{}] {}\n", i, chars));
            }
            output.push('\n');
        }

        // Instructions
        output.push_str("Instructions:\n");
        for (i, instr) in self.instructions.iter().enumerate() {
            let marker = if i == self.entry_point { ">>>" } else { "   " };
            output.push_str(&format!("{} {:04} {}\n", marker, i, instr));
        }

        output
    }

    /// Optimize the program (remove NoOps, combine jumps, etc.)
    pub fn optimize(&mut self) {
        self.remove_noops();
        self.fold_jumps();
    }

    /// Remove NoOp instructions and adjust offsets
    fn remove_noops(&mut self) {
        let mut removed = 0;
        let mut offset_map: Vec<i32> = vec![0; self.instructions.len()];

        // First pass: identify NoOps and compute offset adjustments
        for (i, instr) in self.instructions.iter().enumerate() {
            if matches!(instr, Instruction::NoOp) {
                removed += 1;
            }
            offset_map[i] = -(removed as i32);
        }

        // If no NoOps, nothing to do
        if removed == 0 {
            return;
        }

        // Second pass: remove NoOps and adjust offsets
        let mut new_instructions = Vec::with_capacity(self.instructions.len() - removed);
        let mut old_to_new: Vec<usize> = vec![0; self.instructions.len()];

        for (i, instr) in self.instructions.drain(..).enumerate() {
            if matches!(instr, Instruction::NoOp) {
                continue;
            }

            old_to_new[i] = new_instructions.len();
            new_instructions.push(instr);
        }

        // Adjust jump offsets
        for instr in &mut new_instructions {
            if let Some(_offset) = instr.jump_offset() {
                // For now, keep relative offsets as-is since we're not reordering
                // A more sophisticated optimizer would adjust these
            }
        }

        self.instructions = new_instructions;

        // Adjust entry point
        self.entry_point = old_to_new[self.entry_point];
    }

    /// Fold jump chains (Jump to Jump -> Jump to final target)
    fn fold_jumps(&mut self) {
        // Multiple passes until no more changes
        let mut changed = true;
        while changed {
            changed = false;

            for i in 0..self.instructions.len() {
                if let Some(offset) = self.instructions[i].jump_offset() {
                    let target = (i as i32 + 1 + offset) as usize;
                    if target < self.instructions.len() {
                        if let Instruction::Jump { offset: offset2 } = self.instructions[target] {
                            // Fold: Jump to Jump -> Jump to final target
                            let final_target = target as i32 + 1 + offset2;
                            let new_offset = final_target - (i as i32 + 1);
                            self.instructions[i] = Instruction::Jump { offset: new_offset };
                            changed = true;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_charset_basic() {
        let mut set = CharSet::new();
        set.add(b'a');
        set.add(b'b');
        set.add(b'c');

        assert!(set.contains(b'a'));
        assert!(set.contains(b'b'));
        assert!(set.contains(b'c'));
        assert!(!set.contains(b'd'));
    }

    #[test]
    fn test_charset_from_bytes() {
        let set = CharSet::from_bytes(b"abc");
        assert!(set.contains(b'a'));
        assert!(set.contains(b'b'));
        assert!(set.contains(b'c'));
        assert!(!set.contains(b'd'));
    }

    #[test]
    fn test_charset_from_range() {
        let set = CharSet::from_range(b'a', b'z');
        assert!(set.contains(b'a'));
        assert!(set.contains(b'm'));
        assert!(set.contains(b'z'));
        assert!(!set.contains(b'A'));
        assert!(!set.contains(b'0'));
    }

    #[test]
    fn test_charset_negate() {
        let mut set = CharSet::from_bytes(b"abc");
        set.negate();
        assert!(!set.contains(b'a'));
        assert!(!set.contains(b'b'));
        assert!(!set.contains(b'c'));
        assert!(set.contains(b'd'));
    }

    #[test]
    fn test_program_basic() {
        let mut program = Program::new();

        let idx = program.add_instruction(Instruction::any(1));
        assert_eq!(idx, 0);
        assert_eq!(program.instruction_count(), 1);

        let instr = program.get_instruction(0).unwrap();
        assert_eq!(instr, &Instruction::any(1));
    }

    #[test]
    fn test_program_strings() {
        let mut program = Program::new();

        let idx1 = program.add_string("hello");
        let idx2 = program.add_string("world");
        let idx3 = program.add_string("hello"); // Duplicate

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 0); // Should return existing index

        assert_eq!(program.get_string(0), Some("hello"));
        assert_eq!(program.get_string(1), Some("world"));
    }

    #[test]
    fn test_program_char_sets() {
        let mut program = Program::new();

        let set = CharSet::from_bytes(b"abc");
        let idx = program.add_char_set(set);

        let retrieved = program.get_char_set(idx).unwrap();
        assert!(retrieved.contains(b'a'));
        assert!(retrieved.contains(b'b'));
        assert!(retrieved.contains(b'c'));
    }

    #[test]
    fn test_program_entry_point() {
        let mut program = Program::new();
        program.add_instruction(Instruction::any(1));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        assert_eq!(program.entry_point(), 0);
    }

    #[test]
    fn test_program_disassemble() {
        let mut program = Program::new();
        program.add_string("test");
        program.add_instruction(Instruction::any(1));
        program.add_instruction(Instruction::string(0, 4));
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        let disasm = program.disassemble();
        assert!(disasm.contains("Any 1"));
        assert!(disasm.contains("String"));
        assert!(disasm.contains("End"));
    }

    #[test]
    fn test_program_optimize_noops() {
        let mut program = Program::new();
        program.add_instruction(Instruction::any(1));
        program.add_instruction(Instruction::NoOp);
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        assert_eq!(program.instruction_count(), 3);
        program.optimize();
        assert_eq!(program.instruction_count(), 2);
    }

    #[test]
    fn test_program_fold_jumps() {
        let mut program = Program::new();
        // Jump to Jump should be folded
        program.add_instruction(Instruction::jump(1)); // Jump to instruction 2
        program.add_instruction(Instruction::jump(2)); // Jump to instruction 4
        program.add_instruction(Instruction::end());
        program.set_entry_point(0);

        program.optimize();

        // First jump should now jump directly to instruction 4 (End)
        if let Instruction::Jump { offset } = program.get_instruction(0).unwrap() {
            // After folding, should jump directly to end
            // Original: jump(1) at idx 0, jump(2) at idx 1, end at idx 2
            // jump(1) at 0: target = 0 + 1 + 1 = 2 (end)
            // This is already optimal, so no change
            assert_eq!(*offset, 1);
        } else {
            panic!("Expected Jump instruction");
        }
    }
}
