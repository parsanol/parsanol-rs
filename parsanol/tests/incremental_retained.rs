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
fn trees_equal(
    a_arena: &AstArena,
    a_node: &AstNode,
    b_arena: &AstArena,
    b_node: &AstNode,
) -> bool {
    match (a_node, b_node) {
        (AstNode::Nil, AstNode::Nil) => true,
        (AstNode::Bool(x), AstNode::Bool(y)) => x == y,
        (AstNode::Int(x), AstNode::Int(y)) => x == y,
        (AstNode::Float(x), AstNode::Float(y)) => x == y,
        (
            AstNode::StringRef {
                pool_index: x,
            },
            AstNode::StringRef {
                pool_index: y,
            },
        ) => a_arena.get_string(*x as usize) == b_arena.get_string(*y as usize),
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
                xs.iter().zip(ys.iter()).all(|((xk, xv), (yk, yv))| {
                    xk == yk && trees_equal(a_arena, xv, b_arena, yv)
                })
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
        (Ok(rt), Ok(rr)) => assert!(
            trees_equal(session_arena, rt, ref_arena, rr),
            "edit {step} (offset {offset}, op {op}): retained tree diverged from full reparse"
        ),
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
            retained.as_ref().map(|r| &r.ast).map_err(|e| format!("{e:?}")),
            &ref_arena,
            reference.as_ref().map_err(|e| format!("{e:?}")),
        );
    }
    assert_eq!(boundary_edits, 1, "forced a boundary edit at offset 0");
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
