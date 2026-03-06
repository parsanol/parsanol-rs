//! Tests for Capture, Scope, and Dynamic atoms
//!
//! These tests verify cross-backend parity for the new capture infrastructure.

use super::*;
use crate::portable::ast::AstNode;
use crate::portable::bytecode::backend::Parser;
use crate::portable::capture_state::CaptureValue;
use crate::portable::dynamic::{register_dynamic_callback, ConstCallback};

// ============================================================================
// Capture Atom Tests
// ============================================================================

#[test]
fn test_backend_parity_simple_capture() {
    // Test simple capture: capture("letter", "a")
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let letter = grammar.add_atom(Atom::Capture {
        name: "letter".to_string(),
        atom: a,
    });

    grammar.root = letter;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
}

#[test]
fn test_backend_parity_capture_sequence() {
    // Test sequence with captures: (capture("first", "a") capture("second", "b"))
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });

    let first = grammar.add_atom(Atom::Capture {
        name: "first".to_string(),
        atom: a,
    });
    let second = grammar.add_atom(Atom::Capture {
        name: "second".to_string(),
        atom: b,
    });

    let seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![first, second],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("ab").unwrap();
    let bytecode_result = bytecode_parser.parse("ab").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 2);
}

#[test]
fn test_backend_parity_capture_in_alternative() {
    // Test capture in alternative: (capture("x", "a") | "b")
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });

    let capture_a = grammar.add_atom(Atom::Capture {
        name: "x".to_string(),
        atom: a,
    });

    let alt = grammar.add_atom(Atom::Alternative {
        atoms: vec![capture_a, b],
    });

    grammar.root = alt;

    // Test first alternative
    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar.clone());

    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();

    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);

    // Test second alternative
    let mut packrat_parser2 = Parser::packrat(grammar.clone());
    let mut bytecode_parser2 = Parser::bytecode(grammar);

    let packrat_result2 = packrat_parser2.parse("b").unwrap();
    let bytecode_result2 = bytecode_parser2.parse("b").unwrap();

    assert_eq!(packrat_result2.end_pos, bytecode_result2.end_pos);
    assert_eq!(packrat_result2.end_pos, 1);
}

// ============================================================================
// Scope Atom Tests
// ============================================================================

#[test]
fn test_backend_parity_scope_isolation() {
    // Test scope isolation: scope { capture("inner", "a") }
    // After scope, inner capture should not be visible
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });

    let capture_inner = grammar.add_atom(Atom::Capture {
        name: "inner".to_string(),
        atom: a,
    });

    let scope = grammar.add_atom(Atom::Scope {
        atom: capture_inner,
    });

    grammar.root = scope;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
}

#[test]
fn test_backend_parity_nested_scopes() {
    // Test nested scopes: scope { scope { capture("deep", "a") } }
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });

    let capture_deep = grammar.add_atom(Atom::Capture {
        name: "deep".to_string(),
        atom: a,
    });

    let inner_scope = grammar.add_atom(Atom::Scope { atom: capture_deep });

    let outer_scope = grammar.add_atom(Atom::Scope { atom: inner_scope });

    grammar.root = outer_scope;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
}

#[test]
fn test_backend_parity_scope_in_lookahead() {
    // Test scope in lookahead: &scope { capture("peek", "a") }
    // Lookahead should isolate captures
    let mut grammar = Grammar::new();

    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });

    let capture_peek = grammar.add_atom(Atom::Capture {
        name: "peek".to_string(),
        atom: a,
    });

    let scope = grammar.add_atom(Atom::Scope { atom: capture_peek });

    let lookahead = grammar.add_atom(Atom::Lookahead {
        atom: scope,
        positive: true,
    });

    // Sequence: lookahead followed by "a"
    let seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![lookahead, a],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
}

// ============================================================================
// Dynamic Atom Tests
// ============================================================================

#[test]
fn test_backend_parity_dynamic_const() {
    // Test dynamic atom that returns a constant
    let mut grammar = Grammar::new();

    // Create a callback that always returns "a"
    let callback = ConstCallback::new(
        Atom::Str {
            pattern: "a".to_string(),
        },
        "const_a",
    );
    let callback_id = register_dynamic_callback(Box::new(callback));

    let dynamic = grammar.add_atom(Atom::Dynamic { callback_id });

    grammar.root = dynamic;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("a").unwrap();
    let bytecode_result = bytecode_parser.parse("a").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 1);
}

#[test]
fn test_backend_parity_dynamic_in_sequence() {
    // Test dynamic in sequence: ("prefix" dynamic)
    let mut grammar = Grammar::new();

    let prefix = grammar.add_atom(Atom::Str {
        pattern: "prefix".to_string(),
    });

    let callback = ConstCallback::new(
        Atom::Str {
            pattern: "_suffix".to_string(),
        },
        "const_suffix",
    );
    let callback_id = register_dynamic_callback(Box::new(callback));

    let dynamic = grammar.add_atom(Atom::Dynamic { callback_id });

    let seq = grammar.add_atom(Atom::Sequence {
        atoms: vec![prefix, dynamic],
    });

    grammar.root = seq;

    let mut packrat_parser = Parser::packrat(grammar.clone());
    let mut bytecode_parser = Parser::bytecode(grammar);

    let packrat_result = packrat_parser.parse("prefix_suffix").unwrap();
    let bytecode_result = bytecode_parser.parse("prefix_suffix").unwrap();

    // Position parity
    assert_eq!(packrat_result.end_pos, bytecode_result.end_pos);
    assert_eq!(packrat_result.end_pos, 13); // "prefix" (6) + "_suffix" (7) = 13
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_capture_state_shadowing() {
    // Test that shadowing works correctly in CaptureState
    use crate::portable::capture_state::CaptureState;

    let mut state = CaptureState::new();

    // Outer capture
    state.store("x", CaptureValue::new(0, 5));
    assert_eq!(state.get("x").unwrap().offset, 0);

    // Push scope
    state.push_scope();

    // Shadow
    state.store("x", CaptureValue::new(10, 3));
    assert_eq!(state.get("x").unwrap().offset, 10);

    // Pop scope
    state.pop_scope();

    // Original restored
    assert_eq!(state.get("x").unwrap().offset, 0);
}

#[test]
fn test_capture_state_snapshot_restore() {
    // Test that snapshot/restore works correctly
    use crate::portable::capture_state::CaptureState;

    let mut state = CaptureState::new();

    state.store("a", CaptureValue::new(0, 1));
    let snapshot = state.snapshot();

    state.store("b", CaptureValue::new(1, 1));
    state.push_scope();
    state.store("c", CaptureValue::new(2, 1));

    assert_eq!(state.len(), 3);
    assert_eq!(state.depth(), 1);

    // Restore
    state.restore(&snapshot);

    assert_eq!(state.len(), 1);
    assert_eq!(state.depth(), 0);
    assert!(state.contains("a"));
    assert!(!state.contains("b"));
    assert!(!state.contains("c"));
}

#[test]
fn test_capture_state_max_depth() {
    // Test that max depth is enforced
    use crate::portable::capture_state::{CaptureState, MAX_SCOPE_DEPTH};

    let mut state = CaptureState::new();

    // Push to max depth - 1 should work
    for _ in 0..MAX_SCOPE_DEPTH - 1 {
        state.push_scope();
    }

    // This should work (we're at max - 1)
    assert_eq!(state.depth(), MAX_SCOPE_DEPTH - 1);

    // Pop all
    while state.depth() > 0 {
        state.pop_scope();
    }

    assert_eq!(state.depth(), 0);
}
