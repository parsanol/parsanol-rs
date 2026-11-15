//! The pending diagnostics surface (rs#498): the v2 contracts and the
//! legacy-equivalence gate. The legacy surface's pointer-identity and
//! ordered-buffer contracts are legacy-surface details; the pending
//! surface pins the observable guarantees — deepest-set content and
//! order, bounded descending ranks, frame value semantics, memo replay
//! equality — and, decisively, byte-identical read surfaces against
//! the legacy policy across a failing-input battery.
use super::*;

fn literal(grammar: &mut Grammar, value: &str) -> usize {
    grammar.add_atom(Atom::Str {
        pattern: value.into(),
    })
}

fn seq(grammar: &mut Grammar, atoms: Vec<usize>) -> usize {
    grammar.add_atom(Atom::Sequence { atoms })
}

fn alt(grammar: &mut Grammar, atoms: Vec<usize>) -> usize {
    grammar.add_atom(Atom::Alternative { atoms })
}

/// Build the same grammar shape on twin parsers (legacy + pending)
/// and hand both to the body; the borrows stay local.
fn with_twins(
    input: &'static str,
    build: impl Fn(&mut Grammar) -> usize,
    body: impl FnOnce(&mut PortableParser<'_>, &mut PortableParser<'_>),
) {
    let mut legacy_grammar = Grammar::new();
    let root = build(&mut legacy_grammar);
    legacy_grammar.root = root;
    let mut pending_grammar = Grammar::new();
    let root = build(&mut pending_grammar);
    pending_grammar.root = root;
    let mut legacy_arena = AstArena::new();
    let mut pending_arena = AstArena::new();
    let mut legacy = PortableParser::new(&legacy_grammar, input, &mut legacy_arena);
    let mut pending = PortableParser::new(&pending_grammar, input, &mut pending_arena);
    pending
        .set_diagnostics_policy(DiagnosticsPolicy::Pending)
        .expect("pending policy");
    body(&mut legacy, &mut pending);
}

fn assert_surfaces_identical(legacy: &PortableParser, pending: &PortableParser) {
    assert_eq!(
        legacy.failure_diagnostics(),
        pending.failure_diagnostics(),
        "failure_diagnostics diverged"
    );
    assert_eq!(
        legacy.failure_wire(),
        pending.failure_wire(),
        "failure_wire diverged"
    );
    assert_eq!(
        legacy.failure_ranks(),
        pending.failure_ranks(),
        "failure_ranks diverged"
    );
}

/// The decisive gate: every battery shape produces byte-identical
/// failure surfaces under both policies, in both compatibility modes.
#[test]
fn pending_surfaces_match_legacy_across_the_failing_battery() {
    type BatteryShape = &'static [(&'static str, fn(&mut Grammar) -> usize)];
    let battery: BatteryShape = &[
        // literal miss at 0
        ("abc", |g| literal(g, "xyz")),
        // a sequence failing at its second element
        ("abc", |g| {
            let a = literal(g, "a");
            let z = literal(g, "z");
            seq(g, vec![a, z])
        }),
        // an alternative whose last branch fails deepest
        ("ab", |g| {
            let a = literal(g, "a");
            let q = literal(g, "qq");
            let z = literal(g, "zzz");
            alt(g, vec![a, q, z])
        }),
        // a negative lookahead's probe fails, then the parse succeeds
        ("x", |g| {
            let probe = literal(g, "y");
            let look = g.add_atom(Atom::Lookahead {
                atom: probe,
                positive: false,
            });
            let x = literal(g, "x");
            seq(g, vec![look, x])
        }),
        // a positive lookahead probe fails deepest, parse fails
        ("x", |g| {
            let probe = literal(g, "y");
            let look = g.add_atom(Atom::Lookahead {
                atom: probe,
                positive: true,
            });
            let x = literal(g, "x");
            seq(g, vec![look, x])
        }),
        // end-of-input guard (consume_all prefix success)
        ("ab", |g| literal(g, "a")),
        // repeated labels at one position collapse
        ("!!", |g| {
            let q = literal(g, "?");
            alt(g, vec![q, q, q])
        }),
    ];
    for (input, build) in battery {
        for compatibility in [ParserCompatibility::Strict, ParserCompatibility::Parslet] {
            with_twins(input, *build, |legacy, pending| {
                legacy.set_compatibility(compatibility).expect("compat");
                pending.set_compatibility(compatibility).expect("compat");
                let _ = legacy.parse();
                let _ = pending.parse();
                assert_surfaces_identical(legacy, pending);
                // A second parse on the same instance (state persistence
                // parity) must stay identical too.
                let _ = legacy.parse();
                let _ = pending.parse();
                assert_surfaces_identical(legacy, pending);
            });
        }
    }
}

/// The pending batch-vs-scalar replay contract mirrors the legacy
/// ordering test at the value level.
#[test]
fn pending_batch_matches_scalar_replay_order_and_rank_eviction() {
    for seed_count in [0usize, 4, 12] {
        let mut grammar = Grammar::new();
        grammar.root = literal(&mut grammar, "x");
        let mut expected_arena = AstArena::new();
        let mut actual_arena = AstArena::new();
        let mut frame_arena = AstArena::new();
        let mut expected = PortableParser::new(&grammar, "?", &mut expected_arena);
        let mut actual = PortableParser::new(&grammar, "?", &mut actual_arena);
        let mut source = PortableParser::new(&grammar, "?", &mut frame_arena);
        expected
            .set_diagnostics_policy(DiagnosticsPolicy::Pending)
            .expect("pending");
        actual
            .set_diagnostics_policy(DiagnosticsPolicy::Pending)
            .expect("pending");
        source
            .set_diagnostics_policy(DiagnosticsPolicy::Pending)
            .expect("pending");
        for position in 0..seed_count {
            for label in ["existing", "shared", "existing"] {
                expected.note_failure(position, label.into());
                actual.note_failure(position, label.into());
            }
        }
        let labels: Vec<_> = (0..256)
            .flat_map(|i| [format!("label-{i}"), "shared".into()])
            .collect();
        // Scalar replay: the observable ordering contract.
        for (position, batch) in [(3usize, labels.clone()), (20, vec!["first".to_string()])] {
            for label in batch {
                expected.note_failure(position, label);
            }
        }
        // Batch: the source accumulates the same notes, its frame moves in.
        for (position, batch) in [(3usize, labels), (20, vec!["first".to_string()])] {
            for label in batch {
                source.note_failure(position, label);
            }
        }
        let frame = source.take_failure();
        actual.merge_owned_failure(frame);
        assert_eq!(actual.failure_diagnostics(), expected.failure_diagnostics());
        assert_eq!(actual.failure_ranks(), expected.failure_ranks());
    }
}

/// Frame take/restore preserve the label SET across intervening notes
/// (value semantics; the legacy pointer contract is a legacy detail).
#[test]
fn pending_frames_preserve_labels_across_take_restore() {
    let mut grammar = Grammar::new();
    grammar.root = literal(&mut grammar, "x");
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "?", &mut arena);
    parser
        .set_diagnostics_policy(DiagnosticsPolicy::Pending)
        .expect("pending");
    parser.note_failure(4, "parent-cause".into());
    let parent = parser.take_failure();
    parser.note_failure(9, "child-cause".into());
    parser.note_failure(9, "child-cause-2".into());
    parser.restore_failure(parent);
    // The read surfaces materialize at parse end; frame-level tests
    // bridge explicitly.
    parser.sync_failure_surface();
    let (position, labels) = parser
        .failure_diagnostics()
        .expect("restored failure present");
    assert_eq!(position, 4);
    assert_eq!(labels, vec!["parent-cause".to_string()]);
    // The pending store carries the restore too (a frame move, not a
    // materialization).
    let (wire_position, wire_labels) = parser.failure_wire().expect("wire present");
    assert_eq!(wire_position, 4);
    assert_eq!(wire_labels, vec!["parent-cause".to_string()]);
}

/// Pending ranks stay descending, bounded, and merged through frames
/// exactly like the scalar replay.
#[test]
fn pending_ranks_bounded_descending_across_merges() {
    let mut grammar = Grammar::new();
    grammar.root = literal(&mut grammar, "x");
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "?", &mut arena);
    parser
        .set_diagnostics_policy(DiagnosticsPolicy::Pending)
        .expect("pending");
    for position in [4usize, 9, 2, 9, 6, 1, 8, 3, 5, 7, 0, 11] {
        parser.note_failure(position, format!("label-{position}"));
    }
    parser.sync_failure_surface();
    let ranks = parser.failure_ranks();
    assert!(ranks.len() <= MAX_RANKED_FAILURES);
    let positions: Vec<usize> = ranks.iter().map(|(p, _)| *p).collect();
    let mut sorted = positions.clone();
    sorted.sort_by_key(|p| std::cmp::Reverse(*p));
    assert_eq!(positions, sorted, "ranks must stay descending");
    // The deepest entry mirrors the wire.
    let (deepest, labels) = parser.failure_wire().expect("wire");
    assert_eq!(deepest, positions[0]);
    assert_eq!(labels, ranks[0].1);
}

/// A successful parse with retained strict failures surfaces the same
/// retained set under both policies (the strict deepest policy).
#[test]
fn pending_retained_strict_failures_match_legacy_on_success() {
    let build = |g: &mut Grammar| {
        let probe = literal(g, "y");
        let look = g.add_atom(Atom::Lookahead {
            atom: probe,
            positive: false,
        });
        let x = literal(g, "x");
        seq(g, vec![look, x])
    };
    with_twins("x", build, |legacy, pending| {
        let _ = legacy.parse();
        let _ = pending.parse();
        let ok = pending.parse().is_ok();
        assert!(ok, "the battery shape succeeds");
        assert_surfaces_identical(legacy, pending);
    });
}

/// Switching policies resets accumulated diagnostics (no cross-policy
/// leakage), mirroring the compatibility switch discipline.
#[test]
fn pending_policy_switch_resets_state() {
    let mut grammar = Grammar::new();
    grammar.root = literal(&mut grammar, "x");
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, "?", &mut arena);
    parser.note_failure(3, "legacy-note".into());
    assert!(parser.failure_diagnostics().is_some());
    parser
        .set_diagnostics_policy(DiagnosticsPolicy::Pending)
        .expect("switch");
    assert!(parser.failure_diagnostics().is_none());
    assert!(parser.failure_wire().is_none());
    assert!(parser.failure_ranks().is_empty());
    parser
        .set_diagnostics_policy(DiagnosticsPolicy::Legacy)
        .expect("switch back");
    assert!(parser.failure_diagnostics().is_none());
}
