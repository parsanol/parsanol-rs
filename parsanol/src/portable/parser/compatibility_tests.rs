//! Legacy ordering is explicit; strict defaults and effect barriers remain.
use super::*;
use crate::portable::parslet_transform::to_parslet_compatible;
use crate::portable::transform::{ast_to_value, Value};

fn literal(grammar: &mut Grammar, value: &str) -> usize {
    grammar.add_atom(Atom::Str {
        pattern: value.into(),
    })
}

fn named(grammar: &mut Grammar, atom: usize, name: &str) -> usize {
    grammar.add_atom(Atom::Named {
        name: name.into(),
        atom,
    })
}

fn choices(with_tail: bool) -> (Grammar, usize) {
    choices_with_tail_length(with_tail, 1)
}

fn choices_with_tail_length(with_tail: bool, tail_length: usize) -> (Grammar, usize) {
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "a");
    let short = named(&mut grammar, a, "short");
    let ab = literal(&mut grammar, &format!("a{}", "b".repeat(tail_length)));
    let whole = named(&mut grammar, ab, "whole");
    let choice = grammar.add_atom(Atom::Alternative {
        atoms: vec![short, whole],
    });
    let piece = grammar.add_atom(Atom::Entity { atom: choice });
    let bang = literal(&mut grammar, "!");
    let first = grammar.add_atom(Atom::Sequence {
        atoms: vec![piece, bang],
    });
    let mut branches = vec![first, piece];
    if with_tail {
        let first = named(&mut grammar, a, "first");
        let b = literal(&mut grammar, &"b".repeat(tail_length));
        let second = named(&mut grammar, b, "second");
        branches.push(grammar.add_atom(Atom::Sequence {
            atoms: vec![first, second],
        }));
    }
    grammar.root = grammar.add_atom(Atom::Alternative { atoms: branches });
    (grammar, piece)
}

fn parsed_keys(parser: &mut PortableParser<'_>) -> Result<Vec<String>, ParseError> {
    let node = parser.parse()?;
    let shaped = to_parslet_compatible(&node, parser.arena, parser.input);
    match ast_to_value(&shaped, parser.arena, parser.input) {
        Value::Hash(values) => {
            let mut keys: Vec<_> = values.keys().cloned().collect();
            keys.sort();
            Ok(keys)
        }
        other => panic!("expected named result, got {other:?}"),
    }
}

#[test]
fn strict_default_and_explicit_legacy_choose_their_documented_trees() {
    let (grammar, _) = choices(true);
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "ab", &mut arena);
    assert_eq!(parser.compatibility(), ParserCompatibility::Strict);
    assert_eq!(parsed_keys(&mut parser).unwrap(), ["whole"]);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert_eq!(parsed_keys(&mut parser).unwrap(), ["first", "second"]);
    parser
        .set_compatibility(ParserCompatibility::Strict)
        .unwrap();
    assert!(parser.failure_diagnostics().is_none());
    assert_eq!(parsed_keys(&mut parser).unwrap(), ["whole"]);
}

#[test]
fn switching_policy_clears_warmed_successes_failures_and_diagnostics() {
    let (grammar, _) = choices(false);
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "ab", &mut arena);
    assert!(parser.parse().is_ok());
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert!(parser.failure_diagnostics().is_none());
    assert!(parser.parse().is_err());
    assert!(!parser.legacy_cache.is_empty());
    parser
        .set_compatibility(ParserCompatibility::Strict)
        .unwrap();
    assert!(parser.legacy_cache.is_empty());
    assert!(parser.dyn_cache.is_empty());
    assert!(parser.dynamic_outcomes.is_empty());
    assert!(parser.failure_diagnostics().is_none());
    assert!(parser.parse().is_ok());
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert!(parser.failure_diagnostics().is_none());
    assert!(parser.parse().is_err());
}

#[test]
fn legacy_stores_raw_success_before_the_outside_consumption_recheck() {
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "a");
    grammar.root = a;
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "ab", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert!(parser.try_atom_impl(a, 0, true).is_err());
    assert!(parser.legacy_cache.get(&(a, 0)).unwrap().success);
    assert_eq!(parser.try_atom_impl(a, 0, false).unwrap().end_pos, 1);
    assert!(parser.legacy_cache_memory_usage() > 0);
}

#[test]
fn legacy_shares_raw_failures_across_consume_all_without_reexecution() {
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "a");
    let x = literal(&mut grammar, "x");
    let sequence = grammar.add_atom(Atom::Sequence { atoms: vec![a, x] });
    grammar.root = sequence;
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "ab", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    parser.enable_profiling();
    assert!(parser.try_atom_impl(sequence, 0, false).is_err());
    assert!(parser.try_atom_impl(sequence, 0, true).is_err());
    assert_eq!(parser.dispatch_counts().unwrap()[a], 1);
}

#[test]
fn legacy_preserves_capture_reexecution_and_scope_isolation() {
    for policy in [ParserCompatibility::Strict, ParserCompatibility::Parslet] {
        let mut grammar = Grammar::new();
        let a = literal(&mut grammar, "A");
        let capture = grammar.add_atom(Atom::Capture {
            name: "tag".into(),
            atom: a,
        });
        let bang = literal(&mut grammar, "!");
        let b = literal(&mut grammar, "B");
        let first = grammar.add_atom(Atom::Sequence {
            atoms: vec![capture, bang],
        });
        let second = grammar.add_atom(Atom::Sequence {
            atoms: vec![capture, b],
        });
        let root = grammar.add_atom(Atom::Alternative {
            atoms: vec![first, second],
        });
        grammar.root = root;
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "AB", &mut arena);
        parser.set_compatibility(policy).unwrap();
        parser.parse().unwrap();
        assert_eq!(
            parser.capture_state().get("tag").unwrap().get_text("AB"),
            "A"
        );
        drop(parser);
        grammar.root = grammar.add_atom(Atom::Scope { atom: root });
        let mut parser = PortableParser::new(&grammar, "AB", &mut arena);
        parser.set_compatibility(policy).unwrap();
        parser.parse().unwrap();
        assert!(parser.capture_state().get("tag").is_none());
    }
}

#[test]
fn no_memo_reference_walker_rejects_an_explicit_legacy_policy() {
    let (grammar, _) = choices(true);
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new_reference(&grammar, "ab", &mut arena);
    assert!(parser
        .set_compatibility(ParserCompatibility::Parslet)
        .is_err());
    assert_eq!(parser.compatibility(), ParserCompatibility::Strict);
}

#[test]
fn legacy_choice_is_explicit_below_and_above_the_native_vm_threshold() {
    for tail_length in [1, 9000] {
        let (grammar, _) = choices_with_tail_length(true, tail_length);
        let input = format!("a{}", "b".repeat(tail_length));
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);
        assert_eq!(parsed_keys(&mut parser).unwrap(), ["whole"]);
        parser
            .set_compatibility(ParserCompatibility::Parslet)
            .unwrap();
        assert_eq!(parsed_keys(&mut parser).unwrap(), ["first", "second"]);
        // Reuse the warmed legacy memo and verify the captured text,
        // not only the shape of the selected branch.
        let node = parser.parse().unwrap();
        let shaped = to_parslet_compatible(&node, parser.arena, parser.input);
        let Value::Hash(values) = ast_to_value(&shaped, parser.arena, parser.input) else {
            panic!("expected the legacy named captures");
        };
        assert_eq!(values.get("first"), Some(&Value::String("a".into())));
        assert_eq!(
            values.get("second"),
            Some(&Value::String("b".repeat(tail_length)))
        );
    }
}

#[test]
fn switching_policy_discards_previous_capture_state_before_new_seeding() {
    use crate::portable::CaptureValue;
    let (grammar, _) = choices(true);
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "ab", &mut arena);
    parser
        .capture_state_mut()
        .store("stale", CaptureValue::new(0, 1));
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert!(parser.capture_state().get("stale").is_none());
    parser
        .capture_state_mut()
        .store("new", CaptureValue::new(1, 1));
    assert!(parser.capture_state().get("new").is_some());
    parser
        .set_compatibility(ParserCompatibility::Strict)
        .unwrap();
    assert!(parser.capture_state().get("new").is_none());
}

#[test]
fn legacy_scoped_retry_preserves_captures_for_a_dynamic_callback() {
    use crate::portable::dynamic::{register_dynamic_callback, DynamicCallback, DynamicContext};
    struct CapturedSuffix;
    impl DynamicCallback for CapturedSuffix {
        fn resolve(&self, context: &DynamicContext) -> Option<Atom> {
            let captured = context.captures.get("tag")?;
            (captured.get_text(context.input()) == "A").then(|| Atom::Str {
                pattern: "B".into(),
            })
        }
        fn description(&self) -> &str {
            "suffix requiring the retried capture"
        }
    }
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "A");
    let capture = grammar.add_atom(Atom::Capture {
        name: "tag".into(),
        atom: a,
    });
    let bang = literal(&mut grammar, "!");
    let dynamic = grammar.add_atom(Atom::Dynamic {
        callback_id: register_dynamic_callback(Box::new(CapturedSuffix)),
    });
    let failed = grammar.add_atom(Atom::Sequence {
        atoms: vec![capture, bang],
    });
    let retried = grammar.add_atom(Atom::Sequence {
        atoms: vec![capture, dynamic],
    });
    let alternatives = grammar.add_atom(Atom::Alternative {
        atoms: vec![failed, retried],
    });
    grammar.root = grammar.add_atom(Atom::Scope { atom: alternatives });
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "AB", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    parser
        .parse()
        .expect("the callback must receive the replayed capture");
    assert!(parser.capture_state().get("tag").is_none());
    assert!(parser.dynamic_outcomes.is_empty());
}
