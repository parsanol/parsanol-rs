//! Runtime stack and public capture-result regressions.
use super::*;
use crate::portable::dynamic::{register_dynamic_callback, DynamicCallback, DynamicContext};

struct TagTail;
impl DynamicCallback for TagTail {
    fn resolve(&self, _context: &DynamicContext) -> Option<Atom> {
        Some(Atom::Str {
            pattern: ">".into(),
        })
    }
    fn description(&self) -> &str {
        "optional tag tail"
    }
}

fn right_recursive_grammar() -> Grammar {
    let mut grammar = Grammar::new();
    let rule = grammar.add_atom(Atom::Entity { atom: 0 });
    let letter = grammar.add_atom(Atom::Str {
        pattern: "x".into(),
    });
    let leaf = grammar.add_atom(Atom::Named {
        name: "text".into(),
        atom: letter,
    });
    let tail = grammar.add_atom(Atom::Named {
        name: "expression".into(),
        atom: rule,
    });
    let recursive = grammar.add_atom(Atom::Sequence {
        atoms: vec![leaf, tail],
    });
    let opening = grammar.add_atom(Atom::Str {
        pattern: "<".into(),
    });
    let dynamic = grammar.add_atom(Atom::Dynamic {
        callback_id: register_dynamic_callback(Box::new(TagTail)),
    });
    let tag = grammar.add_atom(Atom::Sequence {
        atoms: vec![opening, dynamic],
    });
    let body = grammar.add_atom(Atom::Alternative {
        atoms: vec![tag, recursive, leaf],
    });
    grammar.atoms[rule] = Atom::Entity { atom: body };
    grammar.root = rule;
    grammar
}

#[test]
fn right_recursive_native_walker_accepts_100_and_101_letters() {
    // Debug builds reserve much larger frames. The target regression uses
    // a 1MiB stack in release; debug suites exercise identical semantics with
    // enough headroom and must not abort the whole test process.
    let stack_size = if cfg!(debug_assertions) {
        16 * 1024 * 1024
    } else {
        1024 * 1024
    };
    std::thread::Builder::new()
        .stack_size(stack_size)
        .spawn(|| {
            for policy in [ParserCompatibility::Strict, ParserCompatibility::Parslet] {
                for length in [100, 101] {
                    let grammar = right_recursive_grammar();
                    let input = "x".repeat(length);
                    let mut arena = AstArena::new();
                    let mut parser = PortableParser::new(&grammar, &input, &mut arena);
                    parser.set_compatibility(policy).unwrap();
                    let node = parser.parse().unwrap();
                    drop(parser);
                    // Walk iteratively so the test measures parser recursion,
                    // not an unrelated recursive tree conversion.
                    let mut pending = vec![node];
                    let mut spans = Vec::new();
                    let mut text_count = 0;
                    let mut expression_count = 0;
                    while let Some(node) = pending.pop() {
                        match node {
                            AstNode::InputRef { offset, length } => spans.push((offset, length)),
                            AstNode::Array { pool_index, length } => {
                                pending
                                    .extend(arena.get_array(pool_index as usize, length as usize));
                            }
                            AstNode::Hash { pool_index, length } => {
                                for (key, value) in
                                    arena.get_hash_items(pool_index as usize, length as usize)
                                {
                                    match key.as_str() {
                                        "text" => text_count += 1,
                                        "expression" => expression_count += 1,
                                        other => panic!("unexpected capture {other}"),
                                    }
                                    pending.push(value);
                                }
                            }
                            AstNode::StringRef { .. } => {} // structural sequence tag
                            other => panic!("unexpected node {other:?}"),
                        }
                    }
                    spans.sort_unstable();
                    let expected: Vec<_> = (0..length as u32).map(|offset| (offset, 1)).collect();
                    assert_eq!(spans, expected);
                    assert_eq!(text_count, length);
                    assert_eq!(expression_count, length - 1);
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

fn state_set_grammar() -> Grammar {
    let mut grammar = Grammar::new();
    grammar.root = grammar.add_atom(Atom::StateSet {
        slot: "mode".into(),
        value: Some("chosen".into()),
        expr: None,
    });
    grammar
}

struct StateFragment;
impl DynamicCallback for StateFragment {
    fn resolve(&self, _context: &DynamicContext) -> Option<Atom> {
        None
    }
    fn resolve_fragment(&self, _context: &DynamicContext) -> Option<(Grammar, usize)> {
        let grammar = state_set_grammar();
        let root = grammar.root;
        Some((grammar, root))
    }
    fn description(&self) -> &str {
        "state-writing fragment"
    }
}

#[test]
fn public_results_keep_the_original_capture_snapshot_type_and_values() {
    for policy in [ParserCompatibility::Strict, ParserCompatibility::Parslet] {
        for dynamic in [false, true] {
            for with_end_pos in [false, true] {
                let mut grammar = state_set_grammar();
                if dynamic {
                    grammar.root = grammar.add_atom(Atom::Dynamic {
                        callback_id: register_dynamic_callback(Box::new(StateFragment)),
                    });
                }
                let mut arena = AstArena::new();
                let mut parser = PortableParser::new(&grammar, "", &mut arena);
                parser.set_compatibility(policy).unwrap();
                let result: ParseResult = if with_end_pos {
                    parser.parse_with_end_pos().unwrap()
                } else {
                    parser.try_atom(grammar.root, 0).unwrap()
                };
                // This explicit assignment also pins the unchanged public field type.
                let captures: Option<CaptureState> = result.capture_state;
                let state = captures.expect("the public snapshot must survive conversion");
                assert_eq!(state.get("mode").unwrap().get_text(""), "chosen");
            }
        }
    }
}

#[test]
fn recursive_result_layout_stays_within_one_cache_line() {
    let recursive = std::mem::size_of::<Result<AtomResult, ParseError>>();
    let public = std::mem::size_of::<Result<ParseResult, ParseError>>();
    eprintln!("recursive result bytes={recursive}, public result bytes={public}");
    assert!(recursive <= 64);
}
