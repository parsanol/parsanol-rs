//! Backend tests module
//!
//! Tests are organized into logical categories:
//! - `basic`: Basic backend selection and characteristics tests
//! - `parity`: Tests verifying both backends produce same results
//! - `complex`: Complex grammar tests (JSON-like, arithmetic)
//! - `captures`: Named capture parity tests
//! - `capture_scope_dynamic`: Tests for Capture, Scope, and Dynamic atoms

mod basic;
mod capture_scope_dynamic;
mod captures;
mod complex;
mod parity;

use crate::portable::grammar::{Atom, Grammar};

// ========================================================================
// Shared Grammar Builders
// ========================================================================

pub fn simple_grammar() -> Grammar {
    let mut grammar = Grammar::new();
    grammar.add_atom(Atom::Str {
        pattern: "hello".to_string(),
    });
    grammar.root = 0;
    grammar
}

pub fn sequence_grammar() -> Grammar {
    let mut grammar = Grammar::new();
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    grammar.add_atom(Atom::Sequence { atoms: vec![a, b] });
    grammar.root = 2;
    grammar
}

pub fn alternative_grammar() -> Grammar {
    let mut grammar = Grammar::new();
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    grammar.add_atom(Atom::Alternative { atoms: vec![a, b] });
    grammar.root = 2;
    grammar
}

pub fn repetition_grammar() -> Grammar {
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
    grammar
}

pub fn regex_grammar() -> Grammar {
    let mut grammar = Grammar::new();
    grammar.add_atom(Atom::Re {
        pattern: "[0-9]+".to_string(),
    });
    grammar.root = 0;
    grammar
}

pub fn lookahead_grammar() -> Grammar {
    // Pattern: "a" &"b" "b"
    let mut grammar = Grammar::new();
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    let lookahead = grammar.add_atom(Atom::Lookahead {
        atom: b,
        positive: true,
    });
    let b2 = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    grammar.add_atom(Atom::Sequence {
        atoms: vec![a, lookahead, b2],
    });
    grammar.root = 4;
    grammar
}

pub fn negative_lookahead_grammar() -> Grammar {
    // Pattern: !"a" "b" - match "b" only if not preceded by "a"
    let mut grammar = Grammar::new();
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    let neg_lookahead = grammar.add_atom(Atom::Lookahead {
        atom: a,
        positive: false,
    });
    grammar.add_atom(Atom::Sequence {
        atoms: vec![neg_lookahead, b],
    });
    grammar.root = 3;
    grammar
}

pub fn optional_grammar() -> Grammar {
    // Pattern: "a"? "b"
    let mut grammar = Grammar::new();
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    let b = grammar.add_atom(Atom::Str {
        pattern: "b".to_string(),
    });
    let a_opt = grammar.add_atom(Atom::Repetition {
        atom: a,
        min: 0,
        max: Some(1),
    });
    grammar.add_atom(Atom::Sequence {
        atoms: vec![a_opt, b],
    });
    grammar.root = 3;
    grammar
}

pub fn one_or_more_grammar() -> Grammar {
    // "a"+ - one or more "a"
    let mut grammar = Grammar::new();
    let a = grammar.add_atom(Atom::Str {
        pattern: "a".to_string(),
    });
    grammar.add_atom(Atom::Repetition {
        atom: a,
        min: 1,
        max: None,
    });
    grammar.root = 1;
    grammar
}

/// Build a simple arithmetic grammar: expr = term (('+' | '-') term)*
pub fn arithmetic_grammar() -> Grammar {
    let mut grammar = Grammar::new();

    // Digits
    let digit = grammar.add_atom(Atom::Re {
        pattern: "[0-9]".to_string(),
    });
    let number = grammar.add_atom(Atom::Repetition {
        atom: digit,
        min: 1,
        max: None,
    });

    // Operators
    let plus = grammar.add_atom(Atom::Str {
        pattern: "+".to_string(),
    });
    let minus = grammar.add_atom(Atom::Str {
        pattern: "-".to_string(),
    });
    let op = grammar.add_atom(Atom::Alternative {
        atoms: vec![plus, minus],
    });

    // Whitespace (optional)
    let ws = grammar.add_atom(Atom::Re {
        pattern: "[ \t]".to_string(),
    });
    let ws_opt = grammar.add_atom(Atom::Repetition {
        atom: ws,
        min: 0,
        max: None,
    });

    // expr = ws* number (ws* op ws* number)*
    let op_and_num = grammar.add_atom(Atom::Sequence {
        atoms: vec![ws_opt, op, ws_opt, number],
    });
    let tail = grammar.add_atom(Atom::Repetition {
        atom: op_and_num,
        min: 0,
        max: None,
    });

    let expr = grammar.add_atom(Atom::Sequence {
        atoms: vec![ws_opt, number, tail, ws_opt],
    });

    grammar.root = expr;
    grammar
}
