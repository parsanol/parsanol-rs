//! Instruction set for the bytecode VM
//!
//! This module defines the instruction set for the PEG bytecode VM.
//! The design is inspired by LPeg's instruction set, adapted for Rust
//! and parsanol's grammar representation.
//!
//! # Instruction Categories
//!
//! ## Matching Instructions
//! - `Any`: Match any n characters
//! - `Char`: Match a single byte
//! - `CharSet`: Match a character from a set
//! - `String`: Match a literal string
//! - `Regex`: Match a regex pattern
//!
//! ## Test Instructions (test + conditional jump)
//! - `TestChar`: Test character, jump on failure
//! - `TestSet`: Test set, jump on failure
//! - `TestAny`: Test any, jump on failure
//!
//! ## Control Flow
//! - `Jump`: Unconditional jump
//! - `Call`: Call a rule (push return address)
//! - `Return`: Return from rule
//! - `End`: Successful completion
//!
//! ## Backtracking
//! - `Choice`: Push backtrack point
//! - `Commit`: Pop and continue
//! - `PartialCommit`: Update position and loop
//! - `BackCommit`: Backtrack and jump
//! - `Fail`: Backtrack
//! - `FailTwice`: Backtrack and pop again
//!
//! ## Captures
//! - `OpenCapture`: Start capture
//! - `CloseCapture`: End capture
//! - `FullCapture`: Single-instruction capture
//!
//! ## Predicates
//! - `PredChoice`: Choice for lookahead
//!
//! ## Advanced
//! - `Behind`: Move position backward
//! - `NoOp`: No operation

use std::fmt;

/// Opcode enum representing all VM instructions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    // ============================================================================
    // Matching Instructions
    // ============================================================================

    /// Match any n characters
    ///
    /// Fails if there are fewer than n characters remaining.
    /// `Any { n }` matches exactly n characters.
    Any = 0,

    /// Match a single byte
    ///
    /// Fails if the current byte doesn't match.
    /// `Char { byte }` matches the specified byte.
    Char = 1,

    /// Match a character from a set
    ///
    /// Fails if the current byte is not in the set.
    /// `CharSet { set_idx }` uses the set at index set_idx.
    CharSet = 2,

    /// Match a literal string
    ///
    /// Fails if the string doesn't match at the current position.
    /// `String { str_idx }` uses the string at index str_idx.
    String = 3,

    /// Match a regex pattern
    ///
    /// Fails if the regex doesn't match at the current position.
    /// `Regex { regex_idx }` uses the regex at index regex_idx.
    Regex = 4,

    // ============================================================================
    // Test Instructions (test + conditional jump)
    // ============================================================================

    /// Test character and jump on failure
    ///
    /// If the current character matches, consume it and continue.
    /// Otherwise, jump to the offset.
    TestChar = 5,

    /// Test set and jump on failure
    ///
    /// If the current character is in the set, consume it and continue.
    /// Otherwise, jump to the offset.
    TestSet = 6,

    /// Test any and jump on failure
    ///
    /// If there are at least n characters, consume n and continue.
    /// Otherwise, jump to the offset.
    TestAny = 7,

    // ============================================================================
    // Control Flow
    // ============================================================================

    /// Unconditional jump
    ///
    /// Jump to the instruction at the given offset.
    Jump = 8,

    /// Call a rule
    ///
    /// Push a return frame and jump to the rule's entry point.
    Call = 9,

    /// Return from a rule
    ///
    /// Pop a return frame and jump to the return address.
    Return = 10,

    /// Successful completion
    ///
    /// The parse has succeeded. Return the current result.
    End = 11,

    // ============================================================================
    // Backtracking
    // ============================================================================

    /// Push a backtrack point
    ///
    /// Creates a choice point with an alternative address.
    /// If subsequent parsing fails, the VM will jump to the alternative.
    Choice = 12,

    /// Commit to a choice
    ///
    /// Pop the backtrack point and continue.
    /// The alternative is no longer needed.
    Commit = 13,

    /// Partial commit for loops
    ///
    /// Keep the backtrack point but update the position.
    /// Used for greedy repetition.
    PartialCommit = 14,

    /// Backtrack and jump
    ///
    /// Used for predicate success in negative lookahead.
    BackCommit = 15,

    /// Backtrack
    ///
    /// Fail and try the next alternative.
    Fail = 16,

    /// Backtrack and pop again
    ///
    /// Fail, pop backtrack point, and fail again.
    FailTwice = 17,

    // ============================================================================
    // Captures
    // ============================================================================

    /// Start a capture
    ///
    /// Marks the beginning of a capture region.
    OpenCapture = 18,

    /// End a capture
    ///
    /// Marks the end of a capture region and creates the capture.
    CloseCapture = 19,

    /// Full capture in one instruction
    ///
    /// Optimization for fixed-length captures.
    FullCapture = 20,

    // ============================================================================
    // Predicates
    // ============================================================================

    /// Choice for lookahead predicates
    ///
    /// Special choice that tracks predicate state.
    PredChoice = 21,

    // ============================================================================
    // Advanced
    // ============================================================================

    /// Move position backward
    ///
    /// Used for lookbehind. Moves the position back n characters.
    Behind = 22,

    /// No operation
    ///
    /// Placeholder, removed during optimization.
    NoOp = 23,

    /// Throw an error with a label
    ///
    /// Used for labeled error handling.
    Throw = 24,

    /// Throw with recovery
    ///
    /// Throw an error and attempt recovery.
    ThrowRec = 25,

    /// Span instruction for Set*
    ///
    /// Optimized instruction for matching zero or more characters from a set.
    Span = 26,

    /// Custom atom instruction
    ///
    /// Calls a registered custom atom at runtime.
    Custom = 27,
}

/// Capture kind for capture instructions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CaptureKind {
    /// Simple capture - capture the matched text
    Simple = 0,

    /// Position capture - capture the current position
    Position = 1,

    /// Constant capture - capture a constant value
    Constant = 2,

    /// Named capture - capture with a name
    Named = 3,

    /// Group capture - group multiple captures
    Group = 4,

    /// Range capture - capture start and end positions
    Range = 5,

    /// Action capture - call a runtime function
    Action = 6,
}

/// A single VM instruction
///
/// Instructions are designed to be compact (typically 8-16 bytes) and
/// efficiently dispatchable. The VM uses a tagged union representation
/// for different instruction variants.
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    // ============================================================================
    // Matching Instructions
    // ============================================================================

    /// Match any n characters
    Any {
        /// Number of characters to match
        n: u32,
    },

    /// Match a single byte
    Char {
        /// The byte to match
        byte: u8,
    },

    /// Match a character from a set
    CharSet {
        /// Index into the program's character set table
        set_idx: u32,
    },

    /// Match a literal string
    String {
        /// Index into the program's string table
        str_idx: u32,
        /// Length of the string (for quick check)
        len: u32,
    },

    /// Match a regex pattern
    Regex {
        /// Index into the program's regex table
        regex_idx: u32,
    },

    // ============================================================================
    // Test Instructions
    // ============================================================================

    /// Test character and jump on failure
    TestChar {
        /// The byte to test
        byte: u8,
        /// Jump offset if test fails
        offset: i32,
    },

    /// Test set and jump on failure
    TestSet {
        /// Index into the character set table
        set_idx: u32,
        /// Jump offset if test fails
        offset: i32,
    },

    /// Test any and jump on failure
    TestAny {
        /// Number of characters to test
        n: u32,
        /// Jump offset if test fails
        offset: i32,
    },

    // ============================================================================
    // Control Flow
    // ============================================================================

    /// Unconditional jump
    Jump {
        /// Jump offset (relative to next instruction)
        offset: i32,
    },

    /// Call a rule
    Call {
        /// Jump offset to rule entry
        offset: i32,
    },

    /// Return from a rule
    Return,

    /// Successful completion
    End,

    // ============================================================================
    // Backtracking
    // ============================================================================

    /// Push a backtrack point
    Choice {
        /// Jump offset to alternative
        offset: i32,
    },

    /// Commit to a choice
    Commit {
        /// Jump offset after commit (usually 0)
        offset: i32,
    },

    /// Partial commit for loops
    PartialCommit {
        /// Jump offset to loop start
        offset: i32,
    },

    /// Backtrack and jump
    BackCommit {
        /// Jump offset after backtrack
        offset: i32,
    },

    /// Backtrack
    Fail,

    /// Backtrack and pop again
    FailTwice,

    // ============================================================================
    // Captures
    // ============================================================================

    /// Start a capture
    OpenCapture {
        /// Kind of capture
        kind: CaptureKind,
        /// Index into the key table (for named captures)
        key_idx: u32,
    },

    /// End a capture
    CloseCapture {
        /// Kind of capture
        kind: CaptureKind,
        /// Index into the key table (for named captures)
        key_idx: u32,
    },

    /// Full capture in one instruction
    FullCapture {
        /// Kind of capture
        kind: CaptureKind,
        /// Index into the key table
        key_idx: u32,
    },

    // ============================================================================
    // Predicates
    // ============================================================================

    /// Choice for lookahead predicates
    PredChoice {
        /// Jump offset to alternative
        offset: i32,
    },

    // ============================================================================
    // Advanced
    // ============================================================================

    /// Move position backward
    Behind {
        /// Number of characters to move back
        n: u32,
    },

    /// No operation
    NoOp,

    /// Throw an error with a label
    Throw {
        /// Index into the label table
        label_idx: u32,
    },

    /// Throw with recovery
    ThrowRec {
        /// Index into the label table
        label_idx: u32,
        /// Jump offset to recovery rule
        recovery_offset: i32,
    },

    /// Span instruction for Set*
    Span {
        /// Index into the character set table
        set_idx: u32,
    },

    /// Custom atom invocation
    ///
    /// Invokes a custom atom registered via `register_custom_atom()`.
    /// The custom atom's `parse()` method is called at runtime.
    Custom {
        /// The custom atom ID (registered via `register_custom_atom()`)
        id: u64,
    },
}

impl Instruction {
    /// Get the opcode for this instruction
    #[inline]
    pub fn opcode(&self) -> Opcode {
        match self {
            Instruction::Any { .. } => Opcode::Any,
            Instruction::Char { .. } => Opcode::Char,
            Instruction::CharSet { .. } => Opcode::CharSet,
            Instruction::String { .. } => Opcode::String,
            Instruction::Regex { .. } => Opcode::Regex,
            Instruction::TestChar { .. } => Opcode::TestChar,
            Instruction::TestSet { .. } => Opcode::TestSet,
            Instruction::TestAny { .. } => Opcode::TestAny,
            Instruction::Jump { .. } => Opcode::Jump,
            Instruction::Call { .. } => Opcode::Call,
            Instruction::Return => Opcode::Return,
            Instruction::End => Opcode::End,
            Instruction::Choice { .. } => Opcode::Choice,
            Instruction::Commit { .. } => Opcode::Commit,
            Instruction::PartialCommit { .. } => Opcode::PartialCommit,
            Instruction::BackCommit { .. } => Opcode::BackCommit,
            Instruction::Fail => Opcode::Fail,
            Instruction::FailTwice => Opcode::FailTwice,
            Instruction::OpenCapture { .. } => Opcode::OpenCapture,
            Instruction::CloseCapture { .. } => Opcode::CloseCapture,
            Instruction::FullCapture { .. } => Opcode::FullCapture,
            Instruction::PredChoice { .. } => Opcode::PredChoice,
            Instruction::Behind { .. } => Opcode::Behind,
            Instruction::NoOp => Opcode::NoOp,
            Instruction::Throw { .. } => Opcode::Throw,
            Instruction::ThrowRec { .. } => Opcode::ThrowRec,
            Instruction::Span { .. } => Opcode::Span,
            Instruction::Custom { .. } => Opcode::Custom,
        }
    }

    /// Check if this instruction can fail (requires backtracking support)
    #[inline]
    pub fn can_fail(&self) -> bool {
        matches!(
            self,
            Instruction::Any { .. }
                | Instruction::Char { .. }
                | Instruction::CharSet { .. }
                | Instruction::String { .. }
                | Instruction::Regex { .. }
                | Instruction::TestChar { .. }
                | Instruction::TestSet { .. }
                | Instruction::TestAny { .. }
                | Instruction::Fail
                | Instruction::FailTwice
                | Instruction::Throw { .. }
                | Instruction::ThrowRec { .. }
        )
    }

    /// Check if this instruction is a jump (modifies IP)
    #[inline]
    pub fn is_jump(&self) -> bool {
        matches!(
            self,
            Instruction::Jump { .. }
                | Instruction::Call { .. }
                | Instruction::TestChar { .. }
                | Instruction::TestSet { .. }
                | Instruction::TestAny { .. }
        )
    }

    /// Get the jump offset if this instruction has one
    #[inline]
    pub fn jump_offset(&self) -> Option<i32> {
        match self {
            Instruction::Jump { offset }
            | Instruction::Call { offset }
            | Instruction::TestChar { offset, .. }
            | Instruction::TestSet { offset, .. }
            | Instruction::TestAny { offset, .. }
            | Instruction::Choice { offset }
            | Instruction::Commit { offset }
            | Instruction::PartialCommit { offset }
            | Instruction::BackCommit { offset }
            | Instruction::PredChoice { offset }
            | Instruction::ThrowRec { recovery_offset: offset, .. } => Some(*offset),
            _ => None,
        }
    }

    /// Create an Any instruction
    #[inline]
    pub fn any(n: u32) -> Self {
        Instruction::Any { n }
    }

    /// Create a Char instruction
    #[inline]
    pub fn char(byte: u8) -> Self {
        Instruction::Char { byte }
    }

    /// Create a CharSet instruction
    #[inline]
    pub fn charset(set_idx: u32) -> Self {
        Instruction::CharSet { set_idx }
    }

    /// Create a String instruction
    #[inline]
    pub fn string(str_idx: u32, len: u32) -> Self {
        Instruction::String { str_idx, len }
    }

    /// Create a Regex instruction
    #[inline]
    pub fn regex(regex_idx: u32) -> Self {
        Instruction::Regex { regex_idx }
    }

    /// Create a Jump instruction
    #[inline]
    pub fn jump(offset: i32) -> Self {
        Instruction::Jump { offset }
    }

    /// Create a Call instruction
    #[inline]
    pub fn call(offset: i32) -> Self {
        Instruction::Call { offset }
    }

    /// Create a Return instruction
    #[inline]
    pub fn ret() -> Self {
        Instruction::Return
    }

    /// Create an End instruction
    #[inline]
    pub fn end() -> Self {
        Instruction::End
    }

    /// Create a Choice instruction
    #[inline]
    pub fn choice(offset: i32) -> Self {
        Instruction::Choice { offset }
    }

    /// Create a Commit instruction
    #[inline]
    pub fn commit(offset: i32) -> Self {
        Instruction::Commit { offset }
    }

    /// Create a PartialCommit instruction
    #[inline]
    pub fn partial_commit(offset: i32) -> Self {
        Instruction::PartialCommit { offset }
    }

    /// Create a BackCommit instruction
    #[inline]
    pub fn back_commit(offset: i32) -> Self {
        Instruction::BackCommit { offset }
    }

    /// Create a PredChoice instruction (for lookahead predicates)
    #[inline]
    pub fn pred_choice(offset: i32) -> Self {
        Instruction::PredChoice { offset }
    }

    /// Create a Fail instruction
    #[inline]
    pub fn fail() -> Self {
        Instruction::Fail
    }

    /// Create a FailTwice instruction
    #[inline]
    pub fn fail_twice() -> Self {
        Instruction::FailTwice
    }

    /// Create an OpenCapture instruction
    #[inline]
    pub fn open_capture(kind: CaptureKind, key_idx: u32) -> Self {
        Instruction::OpenCapture { kind, key_idx }
    }

    /// Create a CloseCapture instruction
    #[inline]
    pub fn close_capture(kind: CaptureKind, key_idx: u32) -> Self {
        Instruction::CloseCapture { kind, key_idx }
    }

    /// Create a FullCapture instruction
    #[inline]
    pub fn full_capture(kind: CaptureKind, key_idx: u32) -> Self {
        Instruction::FullCapture { kind, key_idx }
    }

    /// Create a Span instruction
    #[inline]
    pub fn span(set_idx: u32) -> Self {
        Instruction::Span { set_idx }
    }

    /// Create a Custom instruction
    #[inline]
    pub fn custom(id: u64) -> Self {
        Instruction::Custom { id }
    }

    /// Create a Behind instruction
    #[inline]
    pub fn behind(n: u32) -> Self {
        Instruction::Behind { n }
    }

    /// Create a NoOp instruction
    #[inline]
    pub fn noop() -> Self {
        Instruction::NoOp
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instruction::Any { n } => write!(f, "Any {}", n),
            Instruction::Char { byte } => {
                let c = *byte as char;
                if c.is_ascii_graphic() {
                    write!(f, "Char '{}'", c)
                } else {
                    write!(f, "Char 0x{:02x}", byte)
                }
            }
            Instruction::CharSet { set_idx } => write!(f, "CharSet [{}]", set_idx),
            Instruction::String { str_idx, len } => write!(f, "String [{}] (len={})", str_idx, len),
            Instruction::Regex { regex_idx } => write!(f, "Regex [{}]", regex_idx),
            Instruction::TestChar { byte, offset } => {
                let c = *byte as char;
                if c.is_ascii_graphic() {
                    write!(f, "TestChar '{}' -> +{}", c, offset)
                } else {
                    write!(f, "TestChar 0x{:02x} -> +{}", byte, offset)
                }
            }
            Instruction::TestSet { set_idx, offset } => {
                write!(f, "TestSet [{}] -> +{}", set_idx, offset)
            }
            Instruction::TestAny { n, offset } => write!(f, "TestAny {} -> +{}", n, offset),
            Instruction::Jump { offset } => write!(f, "Jump -> +{}", offset),
            Instruction::Call { offset } => write!(f, "Call -> +{}", offset),
            Instruction::Return => write!(f, "Return"),
            Instruction::End => write!(f, "End"),
            Instruction::Choice { offset } => write!(f, "Choice -> +{}", offset),
            Instruction::Commit { offset } => write!(f, "Commit +{}", offset),
            Instruction::PartialCommit { offset } => write!(f, "PartialCommit -> +{}", offset),
            Instruction::BackCommit { offset } => write!(f, "BackCommit -> +{}", offset),
            Instruction::Fail => write!(f, "Fail"),
            Instruction::FailTwice => write!(f, "FailTwice"),
            Instruction::OpenCapture { kind, key_idx } => {
                write!(f, "OpenCapture {:?} [{}]", kind, key_idx)
            }
            Instruction::CloseCapture { kind, key_idx } => {
                write!(f, "CloseCapture {:?} [{}]", kind, key_idx)
            }
            Instruction::FullCapture { kind, key_idx } => {
                write!(f, "FullCapture {:?} [{}]", kind, key_idx)
            }
            Instruction::PredChoice { offset } => write!(f, "PredChoice -> +{}", offset),
            Instruction::Behind { n } => write!(f, "Behind {}", n),
            Instruction::NoOp => write!(f, "NoOp"),
            Instruction::Throw { label_idx } => write!(f, "Throw [{}]", label_idx),
            Instruction::ThrowRec {
                label_idx,
                recovery_offset,
            } => write!(f, "ThrowRec [{}] -> +{}", label_idx, recovery_offset),
            Instruction::Span { set_idx } => write!(f, "Span [{}]", set_idx),
            Instruction::Custom { id } => write!(f, "Custom #{}", id),
        }
    }
}

impl fmt::Display for CaptureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureKind::Simple => write!(f, "simple"),
            CaptureKind::Position => write!(f, "position"),
            CaptureKind::Constant => write!(f, "constant"),
            CaptureKind::Named => write!(f, "named"),
            CaptureKind::Group => write!(f, "group"),
            CaptureKind::Range => write!(f, "range"),
            CaptureKind::Action => write!(f, "action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_opcode() {
        assert_eq!(Instruction::any(1).opcode(), Opcode::Any);
        assert_eq!(Instruction::char(b'a').opcode(), Opcode::Char);
        assert_eq!(Instruction::jump(10).opcode(), Opcode::Jump);
        assert_eq!(Instruction::ret().opcode(), Opcode::Return);
        assert_eq!(Instruction::end().opcode(), Opcode::End);
        assert_eq!(Instruction::fail().opcode(), Opcode::Fail);
    }

    #[test]
    fn test_instruction_can_fail() {
        assert!(Instruction::any(1).can_fail());
        assert!(Instruction::char(b'a').can_fail());
        assert!(Instruction::fail().can_fail());
        assert!(!Instruction::end().can_fail());
        assert!(!Instruction::jump(10).can_fail());
    }

    #[test]
    fn test_instruction_is_jump() {
        assert!(Instruction::jump(10).is_jump());
        assert!(Instruction::call(5).is_jump());
        assert!(!Instruction::end().is_jump());
        assert!(!Instruction::fail().is_jump());
    }

    #[test]
    fn test_jump_offset() {
        assert_eq!(Instruction::jump(10).jump_offset(), Some(10));
        assert_eq!(Instruction::call(5).jump_offset(), Some(5));
        assert_eq!(Instruction::choice(20).jump_offset(), Some(20));
        assert_eq!(Instruction::end().jump_offset(), None);
        assert_eq!(Instruction::fail().jump_offset(), None);
    }

    #[test]
    fn test_instruction_display() {
        assert_eq!(Instruction::any(3).to_string(), "Any 3");
        assert_eq!(Instruction::char(b'a').to_string(), "Char 'a'");
        assert_eq!(Instruction::jump(10).to_string(), "Jump -> +10");
        assert_eq!(Instruction::ret().to_string(), "Return");
        assert_eq!(Instruction::end().to_string(), "End");
        assert_eq!(Instruction::fail().to_string(), "Fail");
    }

    #[test]
    fn test_capture_kind_display() {
        assert_eq!(CaptureKind::Simple.to_string(), "simple");
        assert_eq!(CaptureKind::Named.to_string(), "named");
        assert_eq!(CaptureKind::Position.to_string(), "position");
    }
}
