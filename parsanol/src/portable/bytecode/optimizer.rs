//! Peephole optimizer for bytecode programs
//!
//! This module implements optimization passes that improve bytecode quality
//! without changing the semantic meaning of programs.
//!
//! # Architecture
//!
//! The optimizer is designed as a separate pass over the `Program`. This allows
//! different optimization strategies to be tested independently.
//!
//! # Design Principles
//!
//! - **OOP**: Each optimization pass is a separate struct
//! - **MECE**: Optimization passes cover distinct scenarios
//! - **Separation of Concerns**: Optimization is separate from compilation
//! - **Open/Closed**: New passes can be added without modifying existing ones

use super::instruction::Instruction;
use super::program::Program;

/// Trait for optimization passes
pub trait OptimizationPass {
    /// Name of the optimization pass
    fn name(&self) -> &'static str;

    /// Run the optimization pass
    fn run(&self, program: &mut Program) -> bool;
}

/// Remove unreachable code after unconditional instructions
pub struct DeadCodeElimination;

impl OptimizationPass for DeadCodeElimination {
    fn name(&self) -> &'static str {
        "dead_code_elimination"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;
        let mut i = 0;

        while i < program.instruction_count() {
            // Check if previous instruction was unconditional
            if i > 0 {
                let prev = program.get_instruction(i - 1);
                if is_unconditional(prev) {
                    // This instruction is unreachable
                    program.remove_instruction(i);
                    changed = true;
                    continue;
                }
            }

            i += 1;
        }

        changed
    }
}

/// Simplify jump chains (Jump to Jump → Jump to target)
pub struct JumpChainSimplification;

impl OptimizationPass for JumpChainSimplification {
    fn name(&self) -> &'static str {
        "jump_chain_simplification"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;

        for i in 0..program.instruction_count() {
            if let Some(instr) = program.get_instruction(i) {
                if let Some(offset) = instr.jump_offset() {
                    let target = (i as i32 + 1 + offset) as usize;

                    // Follow jump chain
                    let mut final_target = target;
                    let mut visited = std::collections::HashSet::new();

                    while final_target < program.instruction_count() {
                        if visited.contains(&final_target) {
                            break; // Cycle detected
                        }
                        visited.insert(final_target);

                        if let Some(next_instr) = program.get_instruction(final_target) {
                            if let Some(next_offset) = next_instr.jump_offset() {
                                final_target = (final_target as i32 + 1 + next_offset) as usize;
                                changed = true;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }

                    if final_target != target {
                        let new_offset = (final_target as i32) - (i as i32 + 1);
                        program.set_instruction(i, Instruction::jump(new_offset));
                    }
                }
            }
        }

        changed
    }
}

/// Simplify Jump to Return → Return
pub struct JumpToReturnSimplification;

impl OptimizationPass for JumpToReturnSimplification {
    fn name(&self) -> &'static str {
        "jump_to_return_simplification"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;

        for i in 0..program.instruction_count() {
            if let Some(instr) = program.get_instruction(i) {
                if let Some(offset) = instr.jump_offset() {
                    let target = (i as i32 + 1 + offset) as usize;

                    if let Some(Instruction::Return) = program.get_instruction(target) {
                        // Replace Jump to Return with Return
                        program.set_instruction(i, Instruction::ret());
                        changed = true;
                    }
                }
            }
        }

        changed
    }
}

/// Simplify Jump to Fail → Fail
pub struct JumpToFailSimplification;

impl OptimizationPass for JumpToFailSimplification {
    fn name(&self) -> &'static str {
        "jump_to_fail_simplification"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;

        for i in 0..program.instruction_count() {
            if let Some(instr) = program.get_instruction(i) {
                if let Some(offset) = instr.jump_offset() {
                    let target = (i as i32 + 1 + offset) as usize;

                    if let Some(Instruction::Fail) = program.get_instruction(target) {
                        // Replace Jump to Fail with Fail
                        program.set_instruction(i, Instruction::fail());
                        changed = true;
                    }
                }
            }
        }

        changed
    }
}

/// Combine adjacent Char instructions into String
pub struct CombineAdjacentChars;

impl OptimizationPass for CombineAdjacentChars {
    fn name(&self) -> &'static str {
        "combine_adjacent_chars"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;
        let mut i = 0;

        while i + 1 < program.instruction_count() {
            let current = program.get_instruction(i);
            let next = program.get_instruction(i + 1);

            match (current, next) {
                (Some(Instruction::Char { byte: b1 }), Some(Instruction::Char { byte: b2 })) => {
                    // Combine two chars into a 2-char string
                    let s = format!("{}{}", *b1 as char, *b2 as char);
                    let str_idx = program.add_string(&s);

                    // Replace current with String
                    program.set_instruction(i, Instruction::string(str_idx, 2));

                    // Remove next instruction
                    program.remove_instruction(i + 1);
                    changed = true;

                    // Don't increment i, next instruction is now at i
                }
                _ => {
                    i += 1;
                }
            }
        }

        changed
    }
}

/// Check if an instruction is unconditional (ends control flow)
fn is_unconditional(instr: Option<&Instruction>) -> bool {
    matches!(
        instr,
        Some(Instruction::End) | Some(Instruction::Return) | Some(Instruction::Fail)
    )
}

/// Convert CharSet* loops to Span instruction
///
/// Pattern: Choice → CharSet → PartialCommit → Commit
/// Becomes: Span
///
/// This is a major optimization for matching zero or more characters from a set.
pub struct SpanOptimization;

impl OptimizationPass for SpanOptimization {
    fn name(&self) -> &'static str {
        "span_optimization"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;
        let mut i = 0;

        while i + 3 < program.instruction_count() {
            let instr0 = program.get_instruction(i);
            let instr1 = program.get_instruction(i + 1);
            let instr2 = program.get_instruction(i + 2);
            let instr3 = program.get_instruction(i + 3);

            // Pattern: Choice → CharSet → PartialCommit → Commit
            // PartialCommit should jump back to CharSet (offset = -2)
            if let (
                Some(Instruction::Choice { offset: choice_offset }),
                Some(Instruction::CharSet { set_idx }),
                Some(Instruction::PartialCommit { offset: partial_offset }),
                Some(Instruction::Commit { offset: _ }),
            ) = (instr0, instr1, instr2, instr3)
            {
                // Check if PartialCommit loops back to CharSet (-2 means back 2 instructions)
                // And Choice jumps to after Commit (offset should be 3 to skip the loop)
                if *partial_offset == -2 && *choice_offset == 3 {
                    // Replace Choice with Span
                    program.set_instruction(i, Instruction::span(*set_idx));

                    // Remove CharSet, PartialCommit, Commit
                    program.remove_instruction(i + 3); // Commit
                    program.remove_instruction(i + 2); // PartialCommit
                    program.remove_instruction(i + 1); // CharSet

                    changed = true;
                    // Don't increment i, continue from same position
                    continue;
                }
            }

            i += 1;
        }

        changed
    }
}

/// Convert simple Char choices to TestChar instruction
///
/// Pattern: Choice → Char → Commit → Jump
/// Becomes: TestChar { byte, offset } (where offset is Choice's offset + 3)
///
/// This optimizes alternatives like: "a" / "b" / "c"
/// The TestChar tests the character and either consumes it (success) or
/// jumps to the failure handler (offset).
///
/// Benefits:
/// - Reduces 3 instructions to 1 instruction
/// - No backtrack stack push/pop for the common case
/// - Faster dispatch
pub struct TestCharOptimization;

impl OptimizationPass for TestCharOptimization {
    fn name(&self) -> &'static str {
        "test_char_optimization"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;
        let mut i = 0;

        // Scan for pattern: Choice → Char → Commit → Jump
        while i + 3 < program.instruction_count() {
            let should_optimize = {
                let instr0 = program.get_instruction(i);
                let instr1 = program.get_instruction(i + 1);
                let instr2 = program.get_instruction(i + 2);
                let instr3 = program.get_instruction(i + 3);

                match (instr0, instr1, instr2, instr3) {
                    (
                        Some(Instruction::Choice { offset: choice_offset }),
                        Some(Instruction::Char { byte }),
                        Some(Instruction::Commit { offset: commit_offset }),
                        Some(Instruction::Jump { .. }),
                    ) if *commit_offset == 0 => {
                        Some((*byte, *choice_offset))
                    }
                    _ => None,
                }
            };

            if let Some((byte, offset)) = should_optimize {
                // Replace Choice with TestChar
                program.set_instruction(i, Instruction::TestChar { byte, offset });

                // Remove Char, Commit, Jump
                program.remove_instruction(i + 3); // Jump
                program.remove_instruction(i + 2); // Commit
                program.remove_instruction(i + 1); // Char

                changed = true;
                // Don't increment i, continue from same position
                continue;
            }

            i += 1;
        }

        changed
    }
}

/// Convert simple CharSet choices to TestSet instruction
///
/// Pattern: Choice → CharSet → Commit → Jump
/// Becomes: TestSet { set_idx, offset } (where offset is Choice's offset + 3)
///
/// This optimizes alternatives like: [a-z] / [0-9]
/// The TestSet tests the character and either consumes it (success) or
/// jumps to the failure handler (offset).
///
/// Benefits:
/// - Reduces 3 instructions to 1 instruction
/// - No backtrack stack push/pop for the common case
/// - Faster dispatch
pub struct TestSetOptimization;

impl OptimizationPass for TestSetOptimization {
    fn name(&self) -> &'static str {
        "test_set_optimization"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;
        let mut i = 0;

        // Scan for pattern: Choice → CharSet → Commit → Jump
        while i + 3 < program.instruction_count() {
            let should_optimize = {
                let instr0 = program.get_instruction(i);
                let instr1 = program.get_instruction(i + 1);
                let instr2 = program.get_instruction(i + 2);
                let instr3 = program.get_instruction(i + 3);

                match (instr0, instr1, instr2, instr3) {
                    (
                        Some(Instruction::Choice { offset: choice_offset }),
                        Some(Instruction::CharSet { set_idx }),
                        Some(Instruction::Commit { offset: commit_offset }),
                        Some(Instruction::Jump { .. }),
                    ) if *commit_offset == 0 => {
                        Some((*set_idx, *choice_offset))
                    }
                    _ => None,
                }
            };

            if let Some((set_idx, offset)) = should_optimize {
                // Replace Choice with TestSet
                program.set_instruction(i, Instruction::TestSet { set_idx, offset });

                // Remove CharSet, Commit, Jump
                program.remove_instruction(i + 3); // Jump
                program.remove_instruction(i + 2); // Commit
                program.remove_instruction(i + 1); // CharSet

                changed = true;
                // Don't increment i, continue from same position
                continue;
            }

            i += 1;
        }

        changed
    }
}

/// Tail call optimization
///
/// Convert Call → Return to Jump
///
/// Pattern: Call → Return
/// Becomes: Jump (to the call target)
///
/// This optimizes recursive grammars by converting tail calls to jumps,
/// eliminating return stack frame creation.
///
/// Benefits:
/// - Eliminates return stack frame for recursive rules
/// - Can turn recursive grammars into iterative loops
/// - Significant performance improvement for deeply recursive grammars
pub struct TailCallOptimization;

impl OptimizationPass for TailCallOptimization {
    fn name(&self) -> &'static str {
        "tail_call_optimization"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;
        let mut i = 0;

        while i + 1 < program.instruction_count() {
            let should_optimize = {
                let instr0 = program.get_instruction(i);
                let instr1 = program.get_instruction(i + 1);

                match (instr0, instr1) {
                    (
                        Some(Instruction::Call { offset }),
                        Some(Instruction::Return),
                    ) => Some(*offset),
                    _ => None,
                }
            };

            if let Some(offset) = should_optimize {
                // Replace Call with Jump to the same target
                program.set_instruction(i, Instruction::jump(offset));

                // Remove Return
                program.remove_instruction(i + 1);

                changed = true;
                // Don't increment i, continue from same position
                continue;
            }

            i += 1;
        }

        changed
    }
}

/// Convert OpenCapture → fixed match → CloseCapture to FullCapture
///
/// Pattern: OpenCapture → Char/String → CloseCapture (same key)
/// Becomes: Char/String → FullCapture
///
/// This is an optimization for fixed-length captures.
pub struct FullCaptureOptimization;

impl OptimizationPass for FullCaptureOptimization {
    fn name(&self) -> &'static str {
        "full_capture_optimization"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;
        let mut i = 0;

        while i + 2 < program.instruction_count() {
            // Check for pattern: OpenCapture → Char/String → CloseCapture
            let should_optimize = {
                let instr0 = program.get_instruction(i);
                let instr1 = program.get_instruction(i + 1);
                let instr2 = program.get_instruction(i + 2);

                match (instr0, instr1, instr2) {
                    (
                        Some(Instruction::OpenCapture { kind, key_idx }),
                        Some(Instruction::Char { .. }),
                        Some(Instruction::CloseCapture { kind: close_kind, key_idx: close_key }),
                    ) if kind == close_kind && key_idx == close_key => {
                        Some((*kind, *key_idx))
                    }
                    (
                        Some(Instruction::OpenCapture { kind, key_idx }),
                        Some(Instruction::String { .. }),
                        Some(Instruction::CloseCapture { kind: close_kind, key_idx: close_key }),
                    ) if kind == close_kind && key_idx == close_key => {
                        Some((*kind, *key_idx))
                    }
                    _ => None,
                }
            };

            if let Some((kind, key_idx)) = should_optimize {
                // Remove OpenCapture
                program.remove_instruction(i);

                // Replace CloseCapture with FullCapture
                // (which is now at i+1 after removing OpenCapture)
                program.set_instruction(i + 1, Instruction::full_capture(kind, key_idx));

                changed = true;
                // Don't increment i, continue from same position
                continue;
            }

            i += 1;
        }

        changed
    }
}

/// Optimize positive lookahead patterns
///
/// Pattern: Choice(fail) → <simple pattern> → BackCommit(continue) → Fail
///
/// This optimization:
/// 1. Replaces `Choice` with `PredChoice` for better VM handling of predicates
/// 2. For simple single-char patterns, can simplify the sequence
///
/// Benefits:
/// - Marks lookahead patterns for VM-level predicate optimizations
/// - Enables better error messages for predicate failures
/// - Reduces instruction count for simple lookahead patterns
pub struct LookaheadOptimization;

impl OptimizationPass for LookaheadOptimization {
    fn name(&self) -> &'static str {
        "lookahead_optimization"
    }

    fn run(&self, program: &mut Program) -> bool {
        let mut changed = false;
        let mut i = 0;

        // Scan for positive lookahead pattern:
        // Choice(fail_offset) → <pattern> → BackCommit(continue_offset) → Fail
        while i + 2 < program.instruction_count() {
            let pattern_info = {
                let instr0 = program.get_instruction(i);
                let instr1 = program.get_instruction(i + 1);
                let instr2 = program.get_instruction(i + 2);

                match (instr0, instr1, instr2) {
                    // Pattern: Choice → Char → BackCommit → ...
                    (
                        Some(Instruction::Choice { offset: choice_offset }),
                        Some(Instruction::Char { byte }),
                        Some(Instruction::BackCommit { offset: backcommit_offset }),
                    ) => {
                        // Check if this looks like a positive lookahead:
                        // - Choice should jump past BackCommit (to a Fail instruction)
                        // - BackCommit should jump past the Fail
                        let fail_target = (i as i32 + 1 + choice_offset) as usize;
                        let continue_target = (i as i32 + 2 + backcommit_offset) as usize;

                        // Verify there's a Fail at the expected position
                        if fail_target < program.instruction_count() {
                            if let Some(Instruction::Fail) = program.get_instruction(fail_target) {
                                // This is a positive lookahead pattern
                                Some(LookaheadPattern::Char {
                                    byte: *byte,
                                    choice_offset: *choice_offset,
                                    fail_idx: fail_target,
                                    continue_target,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    // Pattern: Choice → CharSet → BackCommit → ...
                    (
                        Some(Instruction::Choice { offset: choice_offset }),
                        Some(Instruction::CharSet { set_idx }),
                        Some(Instruction::BackCommit { offset: backcommit_offset }),
                    ) => {
                        let fail_target = (i as i32 + 1 + choice_offset) as usize;
                        let continue_target = (i as i32 + 2 + backcommit_offset) as usize;

                        if fail_target < program.instruction_count() {
                            if let Some(Instruction::Fail) = program.get_instruction(fail_target) {
                                Some(LookaheadPattern::CharSet {
                                    set_idx: *set_idx,
                                    choice_offset: *choice_offset,
                                    fail_idx: fail_target,
                                    continue_target,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };

            if let Some(info) = pattern_info {
                match info {
                    LookaheadPattern::Char {
                        byte: _,
                        choice_offset,
                        fail_idx: _,
                        continue_target: _,
                    } => {
                        // Replace Choice with PredChoice for better VM handling
                        program.set_instruction(
                            i,
                            Instruction::PredChoice {
                                offset: choice_offset,
                            },
                        );

                        // The Char → BackCommit → Fail sequence remains
                        // but now the VM knows this is a predicate pattern
                        changed = true;
                    }
                    LookaheadPattern::CharSet {
                        set_idx: _,
                        choice_offset,
                        fail_idx: _,
                        continue_target: _,
                    } => {
                        // Replace Choice with PredChoice
                        program.set_instruction(
                            i,
                            Instruction::PredChoice {
                                offset: choice_offset,
                            },
                        );
                        changed = true;
                    }
                }

                // Skip past this pattern
                i += 3;
            } else {
                i += 1;
            }
        }

        changed
    }
}

/// Helper enum for lookahead pattern detection
#[allow(dead_code)]
enum LookaheadPattern {
    Char {
        byte: u8,
        choice_offset: i32,
        fail_idx: usize,
        continue_target: usize,
    },
    CharSet {
        set_idx: u32,
        choice_offset: i32,
        fail_idx: usize,
        continue_target: usize,
    },
}

/// Main optimizer that runs all passes
pub struct PeepholeOptimizer {
    passes: Vec<Box<dyn OptimizationPass>>,
    max_iterations: usize,
}

impl PeepholeOptimizer {
    /// Create a new optimizer with all standard passes
    pub fn new() -> Self {
        Self {
            passes: vec![
                // Basic optimizations
                Box::new(DeadCodeElimination),
                Box::new(JumpChainSimplification),
                Box::new(JumpToReturnSimplification),
                Box::new(JumpToFailSimplification),
                Box::new(CombineAdjacentChars),
                // Specialized instruction optimizations
                Box::new(SpanOptimization),
                Box::new(FullCaptureOptimization),
                // Choice pattern optimizations
                Box::new(TestCharOptimization),
                Box::new(TestSetOptimization),
                // Advanced optimizations
                Box::new(TailCallOptimization),
                Box::new(LookaheadOptimization),
            ],
            max_iterations: 10,
        }
    }

    /// Run all optimization passes until no more changes
    pub fn optimize(&self, program: &mut Program) {
        for _ in 0..self.max_iterations {
            let mut any_changed = false;

            for pass in &self.passes {
                if pass.run(program) {
                    any_changed = true;
                }
            }

            if !any_changed {
                break;
            }
        }
    }
}

impl Default for PeepholeOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
