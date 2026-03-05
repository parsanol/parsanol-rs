//! Optimizer tests
//!
//! Tests for bytecode optimization passes.

use super::super::instruction::Instruction;
use super::super::program::{CharSet, Program};
use super::*;

#[test]
fn test_dead_code_elimination() {
    let mut program = Program::new();

    // End, followed by unreachable code
    program.add_instruction(Instruction::end());
    program.add_instruction(Instruction::char(b'a')); // Unreachable

    assert_eq!(program.instruction_count(), 2);

    let pass = DeadCodeElimination;
    pass.run(&mut program);

    // Unreachable code should be removed
    assert_eq!(program.instruction_count(), 1);
}

#[test]
fn test_jump_to_return_simplification() {
    let mut program = Program::new();

    // Jump to Return (offset 0 means jump to next instruction)
    program.add_instruction(Instruction::jump(0)); // Jump to instruction 1
    program.add_instruction(Instruction::ret());

    let pass = JumpToReturnSimplification;
    pass.run(&mut program);

    // Jump should be replaced with Return
    assert!(matches!(program.get_instruction(0), Some(Instruction::Return)));
}

#[test]
fn test_jump_to_fail_simplification() {
    let mut program = Program::new();

    // Jump to Fail (offset 0 means jump to next instruction)
    program.add_instruction(Instruction::jump(0)); // Jump to instruction 1
    program.add_instruction(Instruction::fail());

    let pass = JumpToFailSimplification;
    pass.run(&mut program);

    // Jump should be replaced with Fail
    assert!(matches!(program.get_instruction(0), Some(Instruction::Fail)));
}

#[test]
fn test_combine_adjacent_chars() {
    let mut program = Program::new();

    // Two adjacent Char instructions
    program.add_instruction(Instruction::char(b'a'));
    program.add_instruction(Instruction::char(b'b'));
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 3);

    let pass = CombineAdjacentChars;
    pass.run(&mut program);

    // Should be combined into a String
    assert_eq!(program.instruction_count(), 2);
    if let Some(Instruction::String { len, .. }) = program.get_instruction(0) {
        assert_eq!(*len, 2);
    } else {
        panic!("Expected String instruction");
    }
}

#[test]
fn test_peephole_optimizer_all_passes() {
    let mut program = Program::new();

    // Create a program with multiple optimization opportunities
    program.add_instruction(Instruction::char(b'a'));
    program.add_instruction(Instruction::char(b'b'));
    program.add_instruction(Instruction::jump(0)); // Jump to instruction 3 (Return)
    program.add_instruction(Instruction::ret());
    program.add_instruction(Instruction::char(b'x')); // Unreachable

    let optimizer = PeepholeOptimizer::new();
    optimizer.optimize(&mut program);

    // After optimization:
    // 1. Char+Char combined to String "ab"
    // 2. Jump to Return replaced with Return
    // 3. Unreachable Char removed
    // Result: String "ab", Return
    assert_eq!(program.instruction_count(), 2);
}

#[test]
fn test_span_optimization() {
    let mut program = Program::new();

    // Pattern: Choice → CharSet → PartialCommit → Commit
    // This represents [a-z]*
    let set = CharSet::from_bytes(b"abcdefghijklmnopqrstuvwxyz");
    let set_idx = program.add_char_set(set);

    program.add_instruction(Instruction::choice(3)); // Skip to after Commit on failure
    program.add_instruction(Instruction::charset(set_idx));
    program.add_instruction(Instruction::partial_commit(-2)); // Loop back to CharSet
    program.add_instruction(Instruction::commit(0)); // Continue
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 5);

    let pass = SpanOptimization;
    pass.run(&mut program);

    // Should be converted to: Span, End
    assert_eq!(program.instruction_count(), 2);
    assert!(matches!(
        program.get_instruction(0),
        Some(Instruction::Span { .. })
    ));
}

#[test]
fn test_full_capture_optimization_char() {
    let mut program = Program::new();

    // Pattern: OpenCapture → Char → CloseCapture
    use super::super::instruction::CaptureKind;
    let key_idx = program.add_string("digit");

    program.add_instruction(Instruction::open_capture(CaptureKind::Named, key_idx));
    program.add_instruction(Instruction::char(b'a'));
    program.add_instruction(Instruction::close_capture(CaptureKind::Named, key_idx));
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 4);

    let pass = FullCaptureOptimization;
    pass.run(&mut program);

    // Should be converted to: Char, FullCapture, End
    assert_eq!(program.instruction_count(), 3);
    assert!(matches!(program.get_instruction(0), Some(Instruction::Char { .. })));
    assert!(matches!(
        program.get_instruction(1),
        Some(Instruction::FullCapture { .. })
    ));
}

#[test]
fn test_full_capture_optimization_string() {
    let mut program = Program::new();

    // Pattern: OpenCapture → String → CloseCapture
    use super::super::instruction::CaptureKind;
    let key_idx = program.add_string("keyword");

    let str_idx = program.add_string("hello");
    program.add_instruction(Instruction::open_capture(CaptureKind::Named, key_idx));
    program.add_instruction(Instruction::string(str_idx, 5));
    program.add_instruction(Instruction::close_capture(CaptureKind::Named, key_idx));
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 4);

    let pass = FullCaptureOptimization;
    pass.run(&mut program);

    // Should be converted to: String, FullCapture, End
    assert_eq!(program.instruction_count(), 3);
    assert!(matches!(
        program.get_instruction(0),
        Some(Instruction::String { .. })
    ));
    assert!(matches!(
        program.get_instruction(1),
        Some(Instruction::FullCapture { .. })
    ));
}

#[test]
fn test_test_char_optimization() {
    let mut program = Program::new();

    // Pattern: Choice → Char → Commit → Jump
    // This represents: "a" / next_alternative
    program.add_instruction(Instruction::choice(3)); // Skip to after Jump on failure
    program.add_instruction(Instruction::char(b'a'));
    program.add_instruction(Instruction::commit(0)); // Continue
    program.add_instruction(Instruction::jump(10)); // Jump to continuation
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 5);

    let pass = TestCharOptimization;
    pass.run(&mut program);

    // Should be converted to: TestChar, End
    assert_eq!(program.instruction_count(), 2);
    if let Some(Instruction::TestChar { byte, offset }) = program.get_instruction(0) {
        assert_eq!(*byte, b'a');
        assert_eq!(*offset, 3);
    } else {
        panic!("Expected TestChar instruction");
    }
}

#[test]
fn test_test_set_optimization() {
    let mut program = Program::new();

    // Pattern: Choice → CharSet → Commit → Jump
    // This represents: [a-z] / next_alternative
    let set = CharSet::from_range(b'a', b'z');
    let set_idx = program.add_char_set(set);

    program.add_instruction(Instruction::choice(3)); // Skip to after Jump on failure
    program.add_instruction(Instruction::charset(set_idx));
    program.add_instruction(Instruction::commit(0)); // Continue
    program.add_instruction(Instruction::jump(10)); // Jump to continuation
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 5);

    let pass = TestSetOptimization;
    pass.run(&mut program);

    // Should be converted to: TestSet, End
    assert_eq!(program.instruction_count(), 2);
    if let Some(Instruction::TestSet { set_idx: idx, offset }) = program.get_instruction(0) {
        assert_eq!(*idx, set_idx);
        assert_eq!(*offset, 3);
    } else {
        panic!("Expected TestSet instruction");
    }
}

#[test]
fn test_tail_call_optimization() {
    let mut program = Program::new();

    // Pattern: Call → Return
    // This represents a tail call to a rule
    program.add_instruction(Instruction::call(5)); // Call to rule at offset 5
    program.add_instruction(Instruction::ret()); // Return
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 3);

    let pass = TailCallOptimization;
    pass.run(&mut program);

    // Should be converted to: Jump, End
    assert_eq!(program.instruction_count(), 2);
    if let Some(Instruction::Jump { offset }) = program.get_instruction(0) {
        assert_eq!(*offset, 5);
    } else {
        panic!("Expected Jump instruction");
    }
}

#[test]
fn test_lookahead_optimization_char() {
    let mut program = Program::new();

    // Pattern: Choice → Char → BackCommit → Fail
    // This represents positive lookahead: &"a"
    //
    // Layout (matching compiler output):
    // 0: Choice(2)        -> jump to Fail on failure (offset = fail_idx - (choice_idx + 1))
    // 1: Char('a')        -> match 'a'
    // 2: BackCommit(1)    -> restore position, jump past Fail (offset = continue_idx - (backcommit_idx + 1))
    // 3: Fail             -> failure target
    // 4: End              -> continue after lookahead
    program.add_instruction(Instruction::choice(2)); // (3 - (0 + 1)) = 2
    program.add_instruction(Instruction::char(b'a'));
    program.add_instruction(Instruction::back_commit(1)); // (4 - (2 + 1)) = 1
    program.add_instruction(Instruction::fail());
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 5);

    let pass = LookaheadOptimization;
    pass.run(&mut program);

    // Should convert Choice to PredChoice
    assert_eq!(program.instruction_count(), 5); // Same count, just different instruction
    if let Some(Instruction::PredChoice { offset }) = program.get_instruction(0) {
        assert_eq!(*offset, 2);
    } else {
        panic!("Expected PredChoice instruction");
    }
    // Rest should be unchanged
    assert!(matches!(
        program.get_instruction(1),
        Some(Instruction::Char { .. })
    ));
    assert!(matches!(
        program.get_instruction(2),
        Some(Instruction::BackCommit { .. })
    ));
}

#[test]
fn test_lookahead_optimization_charset() {
    let mut program = Program::new();

    // Pattern: Choice → CharSet → BackCommit → Fail
    // This represents positive lookahead: &[a-z]
    let set = CharSet::from_range(b'a', b'z');
    let set_idx = program.add_char_set(set);

    program.add_instruction(Instruction::choice(2));
    program.add_instruction(Instruction::charset(set_idx));
    program.add_instruction(Instruction::back_commit(1));
    program.add_instruction(Instruction::fail());
    program.add_instruction(Instruction::end());

    assert_eq!(program.instruction_count(), 5);

    let pass = LookaheadOptimization;
    pass.run(&mut program);

    // Should convert Choice to PredChoice
    if let Some(Instruction::PredChoice { offset }) = program.get_instruction(0) {
        assert_eq!(*offset, 2);
    } else {
        panic!("Expected PredChoice instruction");
    }
}
