//! Retained-tree parses (TODO.perf/9): the `*_retained` family keeps
//! full container entries across parses because the session owns its
//! output arena. The gate: after every edit, the retained parse's
//! outcome — tree or failure — equals a full cold reparse of the same
//! input.

use parsanol::portable::incremental::{Edit, IncrementalParser};
use parsanol::portable::parser_dsl::{self, GrammarBuilder, ParsletExt as _};
use parsanol::portable::{AstArena, AstNode, Grammar};

/// Deep structural equality of two trees that live in DIFFERENT
/// arenas: pool indices are resolved through their own arena, and
/// only resolved content is compared.
fn trees_equal(a_arena: &AstArena, a_node: &AstNode, b_arena: &AstArena, b_node: &AstNode) -> bool {
    match (a_node, b_node) {
        (AstNode::Nil, AstNode::Nil) => true,
        (AstNode::Bool(x), AstNode::Bool(y)) => x == y,
        (AstNode::Int(x), AstNode::Int(y)) => x == y,
        (AstNode::Float(x), AstNode::Float(y)) => x == y,
        (AstNode::StringRef { pool_index: x }, AstNode::StringRef { pool_index: y }) => {
            a_arena.get_string(*x as usize) == b_arena.get_string(*y as usize)
        }
        (
            AstNode::InputRef {
                offset: xo,
                length: xl,
            },
            AstNode::InputRef {
                offset: yo,
                length: yl,
            },
        ) => xo == yo && xl == yl,
        (
            AstNode::Array {
                pool_index: xp,
                length: xl,
            },
            AstNode::Array {
                pool_index: yp,
                length: yl,
            },
        ) => {
            xl == yl && {
                let xs = a_arena.get_array(*xp as usize, *xl as usize);
                let ys = b_arena.get_array(*yp as usize, *yl as usize);
                xs.iter()
                    .zip(ys.iter())
                    .all(|(x, y)| trees_equal(a_arena, x, b_arena, y))
            }
        }
        (
            AstNode::Hash {
                pool_index: xp,
                length: xl,
            },
            AstNode::Hash {
                pool_index: yp,
                length: yl,
            },
        ) => {
            xl == yl && {
                let xs = a_arena.get_hash_items(*xp as usize, *xl as usize);
                let ys = b_arena.get_hash_items(*yp as usize, *yl as usize);
                xs.iter()
                    .zip(ys.iter())
                    .all(|((xk, xv), (yk, yv))| xk == yk && trees_equal(a_arena, xv, b_arena, yv))
            }
        }
        _ => false,
    }
}

/// KV document: one `key=value;` per pair. Produces Hash nodes
/// (captures) inside a tagged repetition Array — the container
/// shapes the snapshot tier could not retain.
fn kv_grammar() -> Grammar {
    use parser_dsl::{capture, dynamic, ref_, seq, str};
    let pair = seq([
        dynamic(capture("k", parser_dsl::re(r"[a-z0-9]+"))),
        dynamic(str("=")),
        dynamic(capture("v", parser_dsl::re(r"[0-9]+"))),
        dynamic(str(";")),
    ]);
    let document = ref_("pair").repeat(1, None).label("pairs");
    GrammarBuilder::new()
        .rule("document", document)
        .rule("pair", pair)
        .build()
}

fn kv_corpus(pairs: usize) -> String {
    (0..pairs)
        .map(|i| {
            let key: String = std::iter::repeat_n(char::from(b'a' + (i % 26) as u8), 4).collect();
            format!("{key}{i}={i};")
        })
        .collect()
}

fn full_reparse(
    grammar: &Grammar,
    input: &str,
) -> (AstArena, Result<AstNode, parsanol::portable::ParseError>) {
    let mut arena = AstArena::for_input(input.len());
    let tree = IncrementalParser::new(grammar).parse(input, &mut arena);
    (arena, tree)
}

/// The oracle: the retained outcome must equal the full reparse's —
/// trees deep-equal on success, error payloads identical on failure
/// (edits can genuinely invalidate the input; both engines must agree
/// on that too).
fn assert_same_outcome(
    step: u32,
    offset: usize,
    op: u32,
    session_arena: &AstArena,
    retained: Result<&AstNode, String>,
    ref_arena: &AstArena,
    reference: Result<&AstNode, String>,
) {
    match (retained, reference) {
        (Ok(rt), Ok(rr)) => {
            if !trees_equal(session_arena, rt, ref_arena, rr) {
                eprintln!("SPLICE TREE: {:?}", rt);
                eprintln!("REFER TREE: {:?}", rr);
                for (label, arena, node) in [
                    ("splice", session_arena, rt),
                    ("refer", ref_arena, rr),
                ] {
                    match node {
                        AstNode::Array { pool_index, length } => {
                            let items = arena.get_array(*pool_index as usize, *length as usize);
                            eprintln!("{label} root ARRAY: {} items, first={:?} last={:?}", items.len(), items.first(), items.last());
                        }
                        AstNode::Hash { pool_index, length } => {
                            let pairs = arena.get_hash_items(*pool_index as usize, *length as usize);
                            eprintln!("{label} root HASH: {} pairs", pairs.len());
                            for (k, v) in &pairs {
                                if let AstNode::Array { pool_index: ap, length: al } = v {
                                    let inner = arena.get_array(*ap as usize, *al as usize);
                                    eprintln!("  {label} {k}: {} items", inner.len());
                                    for (idx, it) in inner.iter().take(2).enumerate() {
                                        eprintln!("    {label} item[{idx}] = {:?} -> {:?}", it, it.clone());
                                    }
                                } else {
                                    eprintln!("  {label} {k}: non-array {:?}", v);
                                }
                            }
                        }
                        other => eprintln!("{label} root other: {:?}", other),
                    }
                }
                panic!("diverged");
            }
        }
        (Err(re), Err(rf)) => assert_eq!(
            format!("{re:?}"),
            format!("{rf:?}"),
            "edit {step} (offset {offset}, op {op}): retained error diverged from full reparse"
        ),
        (Ok(_), Err(rf)) => panic!(
            "edit {step} (offset {offset}, op {op}): retained parse succeeded but full reparse failed: {rf:?}"
        ),
        (Err(re), Ok(_)) => panic!(
            "edit {step} (offset {offset}, op {op}): retained parse failed but full reparse succeeded: {re:?}"
        ),
    }
}

#[test]
fn retained_matches_full_reparse_across_edit_session() {
    let grammar = kv_grammar();
    let mut session = IncrementalParser::new(&grammar);

    let mut input = kv_corpus(40);
    let tree = session.parse_retained(&input).expect("initial parse");
    let (ref_arena, ref_tree) = full_reparse(&grammar, &input);
    let ref_tree = ref_tree.expect("reference parse");
    assert!(
        trees_equal(session.retained_arena(), &tree, &ref_arena, &ref_tree),
        "initial retained tree differs from full reparse"
    );

    // Deterministic edit session: 40 edits cycling insert / delete /
    // replace at pseudo-random offsets, including a boundary edit at
    // offset 0. Edits may invalidate the input; the oracle is the
    // full reparse's outcome, whatever it is.
    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut boundary_edits = 0;
    for step in 0..40u32 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let op = step % 3;
        let offset = if step == 7 {
            boundary_edits += 1;
            0
        } else {
            (rng as usize) % input.len()
        };
        let (old_len, inserted) = match op {
            0 => (0usize, "xx".to_string()),
            1 => (input.len().saturating_sub(offset).min(3), String::new()),
            _ => (1usize, "zz9".to_string()),
        };
        let mut next = String::with_capacity(input.len() + inserted.len());
        next.push_str(&input[..offset]);
        next.push_str(&inserted);
        next.push_str(&input[(offset + old_len).min(input.len())..]);
        if next.is_empty() {
            continue;
        }
        let edit = Edit {
            offset,
            old_length: old_len,
            new_length: inserted.len(),
        };
        input = next;

        let retained = session.parse_with_edit_retained(&input, edit);
        let (ref_arena, reference) = full_reparse(&grammar, &input);
        assert_same_outcome(
            step,
            offset,
            op,
            session.retained_arena(),
            retained
                .as_ref()
                .map(|r| &r.ast)
                .map_err(|e| format!("{e:?}")),
            &ref_arena,
            reference.as_ref().map_err(|e| format!("{e:?}")),
        );
    }
    assert_eq!(boundary_edits, 1, "forced a boundary edit at offset 0");
}

#[test]
fn retained_matches_full_reparse_across_multi_region_edits() {
    // TODO.sota/7 acceptance: multi-region edits with the differential
    // gate running after every one of them. Kept at 500 edits so the
    // debug-build gate stays CI-sized; the nightly fuzz job can raise
    // the count by parameter.
    let grammar = kv_grammar();
    let mut session = IncrementalParser::new(&grammar);
    let mut input = kv_corpus(80);
    session.parse_retained(&input).expect("initial parse");

    let mut rng: u64 = 0xC0FF_EE00_1234_5678;
    let mut reused = 0usize;
    let mut regions = std::collections::BTreeSet::new();
    for step in 0..500u32 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let offset = (rng as usize) % input.len();
        let current = input.as_bytes()[offset];
        // Same-class replacement keeps the document valid at any offset:
        // digits stay digits, letters stay letters, punctuation is
        // rewritten to itself (a no-op edit still dirties its region).
        let replacement = if current.is_ascii_digit() {
            b'0' + ((rng >> 16) % 10) as u8
        } else if current.is_ascii_alphabetic() {
            b'a' + ((rng >> 16) % 26) as u8
        } else {
            current
        };
        let mut next = input.clone();
        next.replace_range(offset..offset + 1, &(replacement as char).to_string());
        input = next;

        let result = session
            .parse_with_edit_retained(&input, Edit::replace(offset, 1, 1))
            .unwrap_or_else(|error| {
                panic!("edit {step} at {offset}: retained reparse failed: {error:?}")
            });
        reused += result.reused_cache_entries;
        regions.insert(offset / 64);

        let (ref_arena, reference) = full_reparse(&grammar, &input);
        assert_same_outcome(
            step,
            offset,
            2,
            session.retained_arena(),
            Ok(&result.ast),
            &ref_arena,
            reference.as_ref().map_err(|e| format!("{e:?}")),
        );
    }

    assert!(reused > 0, "no cache entries survived across the session");
    assert!(
        regions.len() > 10,
        "edits covered only {} regions",
        regions.len()
    );
    assert!(
        session.retained_arena().memory_usage() < 64 * 1024 * 1024,
        "retained arena exceeded the session budget"
    );
}

#[test]
fn retained_sessions_reuse_containers_and_stay_bounded() {
    let grammar = kv_grammar();
    let mut session = IncrementalParser::new(&grammar);

    let mut input = kv_corpus(60);
    session.parse_retained(&input).expect("initial parse");

    // Replace one digit per edit at a fixed small offset inside the
    // pair-index digits, so every input stays valid and every edit
    // exercises pure reuse conditions.
    let mut total_reused = 0usize;
    let mut rng: u64 = 0xDEAD_BEEF_CAFE_F00D;
    for step in 0..60u32 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        // Edit inside the LAST pair's value: the whole prefix before
        // the edit is retention-eligible, the maximal-reuse shape.
        let offset = input.len() - 2;
        let digit = char::from(b'0' + ((rng >> 8) % 10) as u8);
        let mut next = input.clone();
        next.replace_range(offset..offset + 1, &digit.to_string());
        input = next;

        let result = session
            .parse_with_edit_retained(&input, Edit::replace(offset, 1, 1))
            .expect("retained reparse");
        assert!(
            result.reused_cache_entries > 0,
            "edit {step} reused no cache entries"
        );
        total_reused += result.reused_cache_entries;

        let (ref_arena, reference) = full_reparse(&grammar, &input);
        assert_same_outcome(
            step,
            offset,
            2,
            session.retained_arena(),
            Ok(&result.ast),
            &ref_arena,
            reference.as_ref().map_err(|e| format!("{e:?}")),
        );
    }

    assert!(total_reused > 0);
    // Same-arena validity: the retained tree reads its input back.
    assert_eq!(session.retained_arena().get_input(), input);
    assert!(
        session.retained_arena().memory_usage() < 64 * 1024 * 1024,
        "persistent output arena exceeded the session budget"
    );
}

#[test]
fn root_alternative_retries_branches_that_leave_input() {
    // The TODO.perf/6 item-3 repro: the first alternative matches a
    // strict prefix of the input. The root consume_all check must
    // retry the later branch instead of erroring Incomplete.
    use parsanol::portable::parser_dsl::{choice, dynamic, seq, str};

    let grammar = GrammarBuilder::new()
        .rule(
            "document",
            choice([
                dynamic(str("ab")),
                dynamic(seq([dynamic(str("ab")), dynamic(str("c"))])),
            ]),
        )
        .build();

    let mut arena = AstArena::for_input(3);
    let tree = parsanol::portable::parser::PortableParser::new(&grammar, "abc", &mut arena)
        .parse()
        .expect("root alternative must backtrack into the longer branch");
    let _ = tree;
}

#[test]
fn splice_path_engages_on_repetition_spine_documents() {
    let grammar = kv_grammar();
    let mut session = IncrementalParser::new(&grammar);

    let mut input = kv_corpus(60);
    session.parse_retained(&input).expect("initial parse");

    let mut rng: u64 = 0x5EED_5EED_5EED_0001;
    for _step in 0..10 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        // Edit the last pair's value digit: the cutoff sits near the
        // document end, so the whole prefix is graftable and the
        // splice path engages.
        let offset = input.len() - 2;
        let digit = char::from(b'0' + ((rng >> 8) % 10) as u8);
        let mut next = input.clone();
        next.replace_range(offset..offset + 1, &digit.to_string());
        input = next;
        session
            .parse_with_edit_retained(&input, Edit::replace(offset, 1, 1))
            .expect("retained reparse");
    }

    assert!(
        session.spliced_parses() > 0,
        "the splice path never engaged"
    );
}

#[test]
fn splice_engagement_holds_as_the_document_grows() {
    // rs#258: the next plan's body chain used to come from memo
    // entries. Repetition-body outcomes spanning a trivia drain are
    // never memoized (the drain replays side effects the memo cannot),
    // so the stored body-entry set thinned out as the document grew
    // and engagement collapsed past ~800 pairs. The chain now comes
    // from the root repetition's own iteration trace, which is exact
    // at every size.
    let grammar = kv_grammar();
    for pairs in [500usize, 1200, 2000] {
        let mut session = IncrementalParser::new(&grammar);
        let mut input = kv_corpus(pairs);
        session.parse_retained(&input).expect("initial parse");

        let edits = 3u32;
        for step in 0..edits {
            // Tail edit: every prefix item is retention-eligible and
            // the splice must engage on EVERY edit, including the
            // first (the initial parse now records the trace too).
            let offset = input.len() - 2;
            let digit = char::from(b'0' + ((step + 1) % 10) as u8);
            let mut next = input.clone();
            next.replace_range(offset..offset + 1, &digit.to_string());
            input = next;

            let result = session
                .parse_with_edit_retained(&input, Edit::replace(offset, 1, 1))
                .unwrap_or_else(|error| {
                    panic!("{pairs} pairs, edit {step}: retained reparse failed: {error:?}")
                });

            let (ref_arena, reference) = full_reparse(&grammar, &input);
            assert_same_outcome(
                step,
                offset,
                2,
                session.retained_arena(),
                Ok(&result.ast),
                &ref_arena,
                reference.as_ref().map_err(|e| format!("{e:?}")),
            );
        }

        assert_eq!(
            session.spliced_parses(),
            edits as usize,
            "{pairs} pairs: splice engagement regressed to {}/{}",
            session.spliced_parses(),
            edits
        );
    }
}

/// The compaction primitive behind the session's dead-node
/// reclamation: keep only the trees reachable from `roots`, drop the
/// rest, rewrite every Array/Hash handle. Pinned against a reference
/// copy of the same trees in a second arena — content equality, not
/// pool indices.
#[test]
fn arena_compact_keeps_exactly_the_root_reachable_trees() {
    let grammar = kv_grammar();
    let input = kv_corpus(60);

    let mut arena = AstArena::for_input(input.len());
    arena.set_input(input.clone());
    let mut parser = IncrementalParser::new(&grammar);
    let tree = parser
        .parse(&input, &mut arena)
        .expect("the reference tree parses");

    // Garbage: superseded subtrees the session would have abandoned.
    let (dead_start, dead_len) = arena.store_array(&[AstNode::InputRef {
        offset: 0,
        length: 3,
    }]);
    let (dead_hash_start, dead_hash_len) = arena.store_hash(&[(
        "dead",
        AstNode::InputRef {
            offset: 9,
            length: 1,
        },
    )]);
    let _ = (dead_start, dead_len, dead_hash_start, dead_hash_len);

    let live_before = arena.pool_lens();
    let mut roots = vec![tree.clone()];
    let remap = arena.compact(&mut roots).expect("compaction succeeds");
    let live_after = arena.pool_lens();

    // The garbage is gone; the live trees are byte-identical in
    // content (the roots were rewritten in place).
    assert!(
        live_after.0 + live_after.1 < live_before.0 + live_before.1,
        "compaction dropped nothing: before {live_before:?} after {live_after:?}"
    );
    let (ref_arena, ref_tree) = full_reparse(&grammar, &input);
    let ref_tree = ref_tree.expect("reference parse");
    assert!(
        trees_equal(&arena, &roots[0], &ref_arena, &ref_tree),
        "the compacted tree diverged from the reference"
    );
    // Remapping is consistent: the old indices no longer resolve.
    assert!(
        remap
            .array
            .iter()
            .all(|&m| m == u32::MAX || (m as usize) < live_after.0),
        "array remap points past the compacted pool"
    );
}

/// The observed regression (the lazy-span-shift's pool pass): dead
/// nodes from superseded edits made the pool grow with the SESSION —
/// 44x the live tree at edit 200 of the doc0 trace, shift 310us at
/// the tail. The payback-gated compaction must reclaim at least 3/4
/// of a session's dead while the deep-equality oracle holds on every
/// edit (offset-0 edits invalidate the whole document, so every edit
/// re-derives the full tree — tree-sized dead per edit, the same
/// accumulation shape the doc0 trace exhibited).
#[test]
fn the_retained_session_reclaims_dead_pool_nodes_without_losing_identity() {
    let grammar = kv_grammar();
    let mut session = IncrementalParser::new(&grammar);

    let mut input = kv_corpus(500);
    session.parse_retained(&input).expect("initial parse");
    let (pa0, ph0) = session.retained_arena().pool_lens();
    let live = pa0 + ph0;

    let edits = 60u32;
    for step in 0..edits {
        // Offset 0: the whole document invalidates; the tree
        // re-derives every edit.
        let (old_len, inserted) = match step % 3 {
            0 => (0usize, "xx".to_string()),
            1 => (1usize, String::new()),
            _ => (1usize, "z9".to_string()),
        };
        let mut next = String::with_capacity(input.len() + inserted.len());
        next.push_str(&inserted);
        next.push_str(&input[old_len.min(input.len())..]);
        if next.is_empty() {
            continue;
        }
        let edit = Edit {
            offset: 0,
            old_length: old_len,
            new_length: inserted.len(),
        };
        input = next;

        let retained = session.parse_with_edit_retained(&input, edit);
        let (ref_arena, reference) = full_reparse(&grammar, &input);
        assert_same_outcome(
            step,
            0,
            step % 3,
            session.retained_arena(),
            retained
                .as_ref()
                .map(|r| &r.ast)
                .map_err(|e| format!("{e:?}")),
            &ref_arena,
            reference.as_ref().map_err(|e| format!("{e:?}")),
        );
    }

    // Without reclamation the pool is ~live x (1 + edits) (every edit
    // leaves a dead tree behind). The count-based backstop (16x live)
    // makes the reclaim deterministic in ANY build mode: the pool
    // crests at ~16x live + one edit's dead, then compacts back —
    // bound 20x live. (The time-based payback path compacts far
    // tighter in release; debug builds defer legitimately, which is
    // exactly why the backstop exists.)
    let (pa, ph) = session.retained_arena().pool_lens();
    let no_reclaim = live * (1 + edits as usize);
    assert!(
        pa + ph < 20 * live,
        "dead nodes accumulated unbounded: pool {} + {} vs live {live} (no-reclaim bound {no_reclaim})",
        pa,
        ph
    );
}
