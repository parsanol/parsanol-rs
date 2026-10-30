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
    assert!(matches!(
        parser.try_atom_impl(a, 0, true),
        Err(ParseError::Failed { position: 1 })
    ));
    assert_eq!(parser.failure_diagnostics().unwrap().0, 1);
    assert!(parser.legacy_cache.get(&(a, 0)).unwrap().outcome.success);
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
    let original = parser.failure_diagnostics().unwrap();
    assert_eq!(original.0, 1);
    parser.clear_legacy_failure();
    assert!(parser.try_atom_impl(sequence, 0, true).is_err());
    assert_eq!(parser.failure_diagnostics(), Some(original));
    assert_eq!(parser.dispatch_counts().unwrap()[a], 1);
    assert!(parser.legacy_diagnostic_bytes > 0);
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

#[test]
fn legacy_optional_failure_is_a_raw_failure_reused_by_a_later_prefix() {
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "a");
    let optional = grammar.add_atom(Atom::Repetition {
        atom: a,
        min: 0,
        max: Some(1),
        tag: RepetitionTag::Maybe,
    });
    let b = literal(&mut grammar, "b");
    let sequence = grammar.add_atom(Atom::Sequence {
        atoms: vec![optional, b],
    });
    let branch = named(&mut grammar, sequence, "branch");
    let fallback = named(&mut grammar, b, "fallback");
    grammar.root = grammar.add_atom(Atom::Alternative {
        atoms: vec![optional, branch, fallback],
    });
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "b", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert_eq!(parsed_keys(&mut parser).unwrap(), ["fallback"]);
    assert!(
        !parser
            .legacy_cache
            .get(&(optional, 0))
            .unwrap()
            .outcome
            .success
    );
}

#[test]
fn legacy_bounded_repetition_caches_success_before_outer_consumption_failure() {
    for (min, tag) in [(0, RepetitionTag::Maybe), (1, RepetitionTag::Repetition)] {
        let mut grammar = Grammar::new();
        let a = literal(&mut grammar, "a");
        let bounded = grammar.add_atom(Atom::Repetition {
            atom: a,
            min,
            max: Some(1),
            tag,
        });
        let b = literal(&mut grammar, "b");
        let tail = named(&mut grammar, b, "tail");
        let sequence = grammar.add_atom(Atom::Sequence {
            atoms: vec![bounded, tail],
        });
        grammar.root = grammar.add_atom(Atom::Alternative {
            atoms: vec![bounded, sequence],
        });
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "ab", &mut arena);
        parser
            .set_compatibility(ParserCompatibility::Parslet)
            .unwrap();
        assert_eq!(parsed_keys(&mut parser).unwrap(), ["tail"]);
        assert!(
            parser
                .legacy_cache
                .get(&(bounded, 0))
                .unwrap()
                .outcome
                .success
        );
    }
}

#[test]
fn legacy_choice_keeps_prefix_memo_seeding_with_many_literal_branches() {
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "a");
    let first = named(&mut grammar, a, "first");
    let piece = grammar.add_atom(Atom::Entity { atom: first });
    let bang = literal(&mut grammar, "!");
    let seeded = grammar.add_atom(Atom::Sequence {
        atoms: vec![piece, bang],
    });
    let b = literal(&mut grammar, "b");
    let second = named(&mut grammar, b, "second");
    let prefix = grammar.add_atom(Atom::Sequence {
        atoms: vec![piece, second],
    });
    let ab = literal(&mut grammar, "ab");
    let whole = named(&mut grammar, ab, "whole");
    let mut branches = vec![seeded, piece, prefix, whole];
    for i in 0..12 {
        branches.push(literal(&mut grammar, &format!("c{i}")));
    }
    grammar.root = grammar.add_atom(Atom::Alternative { atoms: branches });
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "ab", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert_eq!(parsed_keys(&mut parser).unwrap(), ["first", "second"]);
    let node = parser.parse().unwrap();
    let shaped = to_parslet_compatible(&node, parser.arena, parser.input);
    let Value::Hash(values) = ast_to_value(&shaped, parser.arena, parser.input) else {
        panic!("expected seeded prefix captures");
    };
    assert_eq!(values.get("first"), Some(&Value::String("a".into())));
    assert_eq!(values.get("second"), Some(&Value::String("b".into())));
}

#[test]
fn legacy_eager_memo_remains_enabled_after_misses_at_earlier_positions() {
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "a");
    let piece = grammar.add_atom(Atom::Repetition {
        atom: a,
        min: 0,
        max: Some(1),
        tag: RepetitionTag::Maybe,
    });
    let b = literal(&mut grammar, "b");
    let branch_sequence = grammar.add_atom(Atom::Sequence {
        atoms: vec![piece, b],
    });
    let branch = named(&mut grammar, branch_sequence, "branch");
    let fallback = named(&mut grammar, b, "fallback");
    let tail = grammar.add_atom(Atom::Alternative {
        atoms: vec![piece, branch, fallback],
    });
    let x = literal(&mut grammar, "x");
    let y = literal(&mut grammar, "y");
    grammar.root = grammar.add_atom(Atom::Sequence {
        atoms: vec![piece, x, piece, y, tail],
    });
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "xyb", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert_eq!(parsed_keys(&mut parser).unwrap(), ["fallback"]);
    assert!(
        !parser
            .legacy_cache
            .get(&(piece, 2))
            .unwrap()
            .outcome
            .success
    );
}

#[test]
fn legacy_discards_failures_in_a_successful_negative_lookahead() {
    for (prefix, input, expected_position) in [("a", "ab?", 1), ("α", "αb?", 2)] {
        for policy in [ParserCompatibility::Strict, ParserCompatibility::Parslet] {
            let mut grammar = Grammar::new();
            let ab = literal(&mut grammar, &format!("{prefix}b"));
            let probe = named(&mut grammar, ab, "probe");
            let c = literal(&mut grammar, "c");
            let inspected = grammar.add_atom(Atom::Sequence {
                atoms: vec![probe, c],
            });
            let absent = grammar.add_atom(Atom::Lookahead {
                atom: inspected,
                positive: false,
            });
            let a = literal(&mut grammar, prefix);
            let actual = named(&mut grammar, a, "a");
            let bang = literal(&mut grammar, "!");
            grammar.root = grammar.add_atom(Atom::Sequence {
                atoms: vec![absent, actual, bang],
            });
            let mut arena = AstArena::new();
            let mut parser = PortableParser::new(&grammar, input, &mut arena);
            parser.set_compatibility(policy).unwrap();
            let expected = expected_position + usize::from(policy == ParserCompatibility::Strict);
            assert!(
                matches!(parser.parse(), Err(ParseError::Failed { position }) if position == expected)
            );
            let diagnostics = parser.failure_diagnostics().unwrap();
            assert_eq!(diagnostics.0, expected);
            if policy == ParserCompatibility::Parslet {
                assert!(diagnostics.1.iter().all(|label| !label.contains('c')));
                // The warmed failure must preserve the selected position.
                parser.clear_legacy_failure();
                assert!(
                    matches!(parser.parse(), Err(ParseError::Failed { position }) if position == expected)
                );
                assert_eq!(parser.failure_diagnostics().unwrap().0, expected);
            }
        }
    }
}

#[test]
fn legacy_discards_speculation_in_successful_optional_and_choice_atoms() {
    for optional in [false, true] {
        let mut grammar = Grammar::new();
        let ab = literal(&mut grammar, "ab");
        let probe = named(&mut grammar, ab, "probe");
        let c = literal(&mut grammar, "c");
        let speculative = grammar.add_atom(Atom::Sequence {
            atoms: vec![probe, c],
        });
        let a = literal(&mut grammar, "a");
        let selected = if optional {
            grammar.add_atom(Atom::Repetition {
                atom: speculative,
                min: 0,
                max: Some(1),
                tag: RepetitionTag::Maybe,
            })
        } else {
            grammar.add_atom(Atom::Alternative {
                atoms: vec![speculative, a],
            })
        };
        let mut atoms = vec![selected];
        if optional {
            atoms.push(a);
        }
        let bang = literal(&mut grammar, "!");
        atoms.push(bang);
        grammar.root = grammar.add_atom(Atom::Sequence { atoms });
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, "ab?", &mut arena);
        parser
            .set_compatibility(ParserCompatibility::Parslet)
            .unwrap();
        assert!(matches!(
            parser.parse(),
            Err(ParseError::Failed { position: 1 })
        ));
        assert_eq!(parser.failure_ranks().len(), 1);
        assert_eq!(parser.failure_diagnostics().unwrap().0, 1);
    }
}

#[test]
fn legacy_failed_negative_lookahead_reports_its_own_position() {
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "a");
    grammar.root = grammar.add_atom(Atom::Lookahead {
        atom: a,
        positive: false,
    });
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "a", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert!(matches!(
        parser.parse(),
        Err(ParseError::Failed { position: 0 })
    ));
    let (position, labels) = parser.failure_diagnostics().unwrap();
    assert_eq!(position, 0);
    assert!(labels
        .iter()
        .any(|label| label.contains("Negative lookahead")));
}

#[test]
fn legacy_outer_consumption_failure_uses_utf8_byte_end_of_raw_success() {
    let mut grammar = Grammar::new();
    let alpha = literal(&mut grammar, "α");
    grammar.root = named(&mut grammar, alpha, "alpha");
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "αb", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert!(matches!(
        parser.parse(),
        Err(ParseError::Failed { position: 2 })
    ));
    assert_eq!(parser.failure_diagnostics().unwrap().0, 2);
    let parsed = parser.try_atom_impl(alpha, 0, false).unwrap();
    assert_eq!(parsed.end_pos, 2);
    assert_eq!(parser.input, "αb");
}

#[test]
fn legacy_failed_memo_diagnostics_count_toward_the_memory_limit() {
    let mut grammar = Grammar::new();
    let a = literal(&mut grammar, "a");
    let missing = literal(&mut grammar, "missing");
    let sequence = grammar.add_atom(Atom::Sequence {
        atoms: vec![a, missing],
    });
    grammar.root = sequence;
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "ab", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert!(parser.parse().is_err());
    assert!(parser.legacy_diagnostic_bytes > 0);
    let total = parser.memory_usage();
    parser.set_max_memory(total - 1);
    assert!(matches!(
        parser.parse(),
        Err(ParseError::MemoryLimitExceeded { .. })
    ));
    parser.set_max_memory(0);
    parser
        .set_compatibility(ParserCompatibility::Strict)
        .unwrap();
    assert_eq!(parser.legacy_diagnostic_bytes, 0);
}

#[test]
fn legacy_repetition_does_not_turn_an_unsupported_native_atom_into_success() {
    let mut grammar = Grammar::new();
    let unsupported = grammar.add_atom(Atom::CustomRef {
        name: "unsupported".into(),
    });
    grammar.root = grammar.add_atom(Atom::Repetition {
        atom: unsupported,
        min: 0,
        max: Some(1),
        tag: RepetitionTag::Maybe,
    });
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "", &mut arena);
    parser
        .set_compatibility(ParserCompatibility::Parslet)
        .unwrap();
    assert!(matches!(
        parser.parse(),
        Err(ParseError::InvalidGrammar { .. })
    ));
    assert!(parser.legacy_cache.is_empty());
}
