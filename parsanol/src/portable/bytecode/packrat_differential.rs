//! Differential tests: the bytecode VM must agree with the packrat
//! tree-walker on success/failure, consumed length, and the materialized
//! AST for every grammar in the battery. This is the gate that must hold
//! before the VM can serve any production parse path.

#![cfg(test)]

use crate::ffi::shared::collapse_ast;
use crate::portable::arena::AstArena;
use crate::portable::bytecode::{compile_bytecode, parse_with_vm};
use crate::portable::grammar::{Atom, Grammar};
use crate::portable::parser::PortableParser;
use crate::portable::parser_dsl;
use crate::portable::parser_dsl::{
    capture, choice, dynamic, re, ref_, seq, GrammarBuilder, ParsletExt,
};
use crate::portable::visitor::{walk, Visitor};
use std::fmt::Write as _;

/// Materializes an AstNode tree into a canonical string so two parses can
/// be compared structurally (values, shapes, and ordering).
struct TreeMaterializer {
    out: String,
}

impl Visitor for TreeMaterializer {
    fn visit_nil(&mut self) {
        self.out.push_str("nil;");
    }

    fn visit_bool(&mut self, value: bool) {
        let _ = write!(self.out, "b:{value};");
    }

    fn visit_int(&mut self, value: i64) {
        let _ = write!(self.out, "i:{value};");
    }

    fn visit_float(&mut self, value: f64) {
        let _ = write!(self.out, "f:{value};");
    }

    fn visit_string_ref(&mut self, pool_index: u32, arena: &AstArena) {
        let _ = write!(self.out, "s:{:?};", arena.get_string(pool_index as usize));
    }

    fn visit_input_ref(&mut self, offset: u32, length: u32, input: &str) {
        let end = (offset + length) as usize;
        let _ = write!(self.out, "in:{:?};", &input[offset as usize..end]);
    }

    fn visit_array_start(&mut self, _: u32, length: u32) {
        let _ = write!(self.out, "[{length}(");
    }

    fn visit_array_end(&mut self, _: u32, _: u32) {
        self.out.push_str(")];");
    }

    fn visit_hash_start(&mut self, _: u32, _: u32) {
        self.out.push('{');
    }

    fn visit_hash_key(&mut self, key: &str) {
        let _ = write!(self.out, "{key:?}->");
    }

    fn visit_hash_end(&mut self, _: u32, _: u32) {
        self.out.push_str("};");
    }
}

fn materialize(root: &crate::portable::ast::AstNode, arena: &AstArena, input: &str) -> String {
    walk(root, arena, input, TreeMaterializer { out: String::new() }).out
}

/// Run both engines over `input` and assert they agree.
fn assert_agree(grammar_json: &str, input: &str) {
    let grammar = Grammar::from_json(grammar_json).expect("grammar compiles");
    let program = compile_bytecode(grammar.clone()).expect("VM compiles grammar");
    assert_agree_compiled(&grammar, &program, input);
}

fn assert_agree_compiled(
    grammar: &Grammar,
    program: &crate::portable::bytecode::Program,
    input: &str,
) {
    let mut packrat_arena = AstArena::new();
    let mut packrat_parser = PortableParser::new(grammar, input, &mut packrat_arena);
    let packrat = packrat_parser.parse_with_end_pos();

    let mut vm_arena = AstArena::new();
    let vm = parse_with_vm(program, input, &mut vm_arena);

    match (&packrat, &vm) {
        (Ok(p), Ok(v)) => {
            assert_eq!(p.end_pos, v.end_pos, "end_pos differs for input {input:?}");
            assert_eq!(
                materialize(&p.value, &packrat_arena, input),
                materialize(&v.value, &vm_arena, input),
                "tree differs for input {input:?}"
            );
        }
        (Err(_), Err(_)) => {}
        (p, v) => panic!("success/failure mismatch for input {input:?}: packrat={p:?} vm={v:?}"),
    }
}

/// Like [`assert_agree`], but compares the trees after `collapse_ast`, the
/// shape every bridge (Ruby, C) delivers. A `1*class` repetition compiles
/// to one Span, whose raw tree holds the run as one value where the walker
/// holds one value per character; collapsing joins both into one slice.
fn assert_agree_collapsed(grammar_json: &str, input: &str) {
    let grammar = Grammar::from_json(grammar_json).expect("grammar compiles");
    let program = compile_bytecode(grammar.clone()).expect("VM compiles grammar");

    let mut packrat_arena = AstArena::new();
    let packrat = PortableParser::new(&grammar, input, &mut packrat_arena).parse_with_end_pos();
    let mut vm_arena = AstArena::new();
    let vm = parse_with_vm(&program, input, &mut vm_arena);

    match (&packrat, &vm) {
        (Ok(p), Ok(v)) => {
            assert_eq!(p.end_pos, v.end_pos, "end_pos differs for input {input:?}");
            let p_tree = collapse_ast(&p.value, &mut packrat_arena);
            let v_tree = collapse_ast(&v.value, &mut vm_arena);
            assert_eq!(
                materialize(&p_tree, &packrat_arena, input),
                materialize(&v_tree, &vm_arena, input),
                "collapsed tree differs for input {input:?}"
            );
        }
        (Err(_), Err(_)) => {}
        (p, v) => panic!("success/failure mismatch for input {input:?}: packrat={p:?} vm={v:?}"),
    }
}

// ---------------------------------------------------------------------------
// Grammar battery
// ---------------------------------------------------------------------------

fn kv_grammar() -> String {
    let value = choice(vec![
        dynamic(seq(vec![
            dynamic(ref_("ident")),
            dynamic(parser_dsl::str(".")),
            dynamic(ref_("ident")),
        ])),
        dynamic(ref_("ident")),
        dynamic(ref_("number")),
    ]);
    let line = seq(vec![
        dynamic(ref_("ident")),
        dynamic(re(r"\s+").ignore()),
        dynamic(parser_dsl::str("=")),
        dynamic(re(r"\s+").ignore()),
        dynamic(ref_("value")),
        dynamic(parser_dsl::str(";")),
    ]);
    GrammarBuilder::new()
        .rule("document", capture("lines", ref_("line").many1()))
        .rule("line", line)
        .rule("value", value)
        .rule("ident", re("[a-zA-Z_][a-zA-Z0-9_]*"))
        .rule("number", re("[0-9]+"))
        .build()
        .to_json()
        .unwrap()
}

fn captures_grammar() -> String {
    let assignment = seq(vec![
        dynamic(capture("name", re("[a-z]+"))),
        dynamic(re(r"\s*=\s*").ignore()),
        dynamic(choice(vec![
            dynamic(capture("number", re("[0-9]+"))),
            dynamic(capture(
                "string",
                seq(vec![
                    dynamic(parser_dsl::str("\"")),
                    dynamic(capture("body", re(r#"[^"]*"#))),
                    dynamic(parser_dsl::str("\"")),
                ]),
            )),
        ])),
        dynamic(ref_("trailer").optional()),
    ]);
    GrammarBuilder::new()
        .rule("root", assignment)
        .rule("trailer", re(r"\s*;"))
        .build()
        .to_json()
        .unwrap()
}

fn lookahead_grammar() -> String {
    let guarded = seq(vec![
        dynamic(parser_dsl::str("if").lookahead()),
        dynamic(capture("kw", re("[a-z]+"))),
        dynamic(re(r"\s+").ignore()),
        dynamic(choice(vec![
            dynamic(capture("num", re("[0-9]+"))),
            dynamic(capture("word", re("[a-z]+"))),
        ])),
    ]);
    GrammarBuilder::new()
        .rule("root", guarded)
        .build()
        .to_json()
        .unwrap()
}

fn nested_repetition_grammar() -> String {
    let element = choice(vec![
        dynamic(seq(vec![
            dynamic(parser_dsl::str("(")),
            dynamic(capture("group", ref_("list"))),
            dynamic(parser_dsl::str(")")),
        ])),
        dynamic(capture("atom", re("[a-z]+"))),
    ]);
    let list = seq(vec![
        dynamic(ref_("element")),
        dynamic(
            seq(vec![
                dynamic(re(r"\s*,\s*").ignore()),
                dynamic(ref_("element")),
            ])
            .many(),
        ),
    ]);
    GrammarBuilder::new()
        .rule("root", ref_("list"))
        .rule("list", list)
        .rule("element", element)
        .build()
        .to_json()
        .unwrap()
}

fn maybe_tag_grammar() -> String {
    let entry = seq(vec![
        dynamic(capture("key", re("[a-z]+"))),
        dynamic(re(r"\s*:\s*").ignore()),
        dynamic(dynamic(ref_("val")).optional()),
    ]);
    GrammarBuilder::new()
        .rule("root", entry)
        .rule("val", re("[0-9]*"))
        .build()
        .to_json()
        .unwrap()
}

#[test]
fn differential_kv_grammar() {
    let g = kv_grammar();
    for input in [
        "a = b;",
        "a = 1;",
        "a = b.c;",
        "a = b;\nc = 2;\nd = e.f;\n",
        "a = ;",
        "= b;",
        "a =",
        "",
        "a = b\nc = d;",
        "ident_x = value_9;",
    ] {
        assert_agree(&g, input);
    }
}

#[test]
fn differential_captures() {
    let g = captures_grammar();
    for input in [
        "x = 1",
        "x = \"hello\"",
        "x = 1 ;",
        "x = \"a b c\"",
        "x =",
        "x",
        "9 = 1",
    ] {
        assert_agree(&g, input);
    }
}

#[test]
fn differential_lookahead() {
    let g = lookahead_grammar();
    for input in ["if 1", "if two", "if", "xx 9", "if 3x", ""] {
        assert_agree(&g, input);
    }
}

#[test]
fn differential_nested_repetition() {
    let g = nested_repetition_grammar();
    for input in [
        "abc",
        "abc, def",
        "(abc, def), ghi",
        "((a), b), (c, d), e",
        "(abc",
        "abc,",
        ",abc",
        "",
    ] {
        assert_agree(&g, input);
    }
}

#[test]
fn differential_maybe_tag() {
    let g = maybe_tag_grammar();
    for input in ["key: 12", "key: ", "key:", "key", ":", "key: x"] {
        assert_agree(&g, input);
    }
}

fn dispatch_grammar() -> String {
    // Branches with provably disjoint lead-byte sets: the compiler
    // emits ByteDispatch for this shape, and the differential asserts
    // the dispatched program matches the tree-walker byte-for-byte.
    let item = choice(vec![
        dynamic(seq(vec![
            dynamic(parser_dsl::str("(")),
            dynamic(capture("group", ref_("list"))),
            dynamic(parser_dsl::str(")")),
        ])),
        dynamic(capture("atom", re("[a-z]+"))),
        dynamic(capture("num", re("[0-9]+"))),
    ]);
    let list = seq(vec![
        dynamic(ref_("item")),
        dynamic(
            seq(vec![
                dynamic(re(r"\s*,\s*").ignore()),
                dynamic(ref_("item")),
            ])
            .many(),
        ),
    ]);
    GrammarBuilder::new()
        .rule("root", ref_("list"))
        .rule("list", list)
        .rule("item", item)
        .build()
        .to_json()
        .unwrap()
}

#[test]
fn differential_lead_byte_dispatch() {
    let g = dispatch_grammar();
    for input in [
        "abc",
        "abc, 42",
        "(abc), 7, (xy, 3), zz",
        "42",
        "(abc",
        "abc,,42",
        "",
        "(",
    ] {
        assert_agree(&g, input);
    }
}

#[test]
fn differential_custom_atom() {
    // Custom atoms have no DSL constructor; the grammar is built through
    // the atom API directly (exactly one Custom atom as an alternative
    // branch against a class regex).
    struct Digits;
    impl crate::portable::custom::CustomAtom for Digits {
        fn parse(&self, input: &str, pos: usize) -> Option<crate::portable::custom::CustomResult> {
            let bytes = input.as_bytes();
            let mut end = pos;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            (end > pos).then(|| crate::portable::custom::CustomResult {
                end_pos: end,
                value: Some(crate::portable::ast::AstNode::InputRef {
                    offset: pos as u32,
                    length: (end - pos) as u32,
                }),
            })
        }

        fn description(&self) -> &str {
            "digits"
        }
    }
    let id = crate::portable::custom::register_custom_atom_auto(Box::new(Digits));

    let mut grammar = Grammar::new();
    let digits = grammar.add_atom(Atom::Custom { id });
    let word = grammar.add_atom(Atom::Re {
        pattern: "[a-z]+".to_string(),
    });
    let alt = grammar.add_atom(Atom::Alternative {
        atoms: vec![digits, word],
    });
    grammar.root = alt;

    let program = compile_bytecode(grammar.clone()).expect("compiles");
    for input in ["123", "abc", "42xyz", "xyz42", ""] {
        let mut packrat_arena = AstArena::new();
        let mut packrat_parser = PortableParser::new(&grammar, input, &mut packrat_arena);
        let packrat = packrat_parser.parse_with_end_pos();

        let mut vm_arena = AstArena::new();
        let vm = parse_with_vm(&program, input, &mut vm_arena);

        match (&packrat, &vm) {
            (Ok(p), Ok(v)) => {
                assert_eq!(p.end_pos, v.end_pos, "end_pos differs for {input:?}");
                assert_eq!(
                    materialize(&p.value, &packrat_arena, input),
                    materialize(&v.value, &vm_arena, input),
                    "tree differs for {input:?}"
                );
            }
            (Err(_), Err(_)) => {}
            (p, v) => panic!("mismatch for {input:?}: packrat={p:?} vm={v:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Selective memoization (#100.1/#90.3): rules that transitively contain
// host calls or capture state must not be memoized; dynamic-free rules
// must be. Trees must agree with the packrat engine either way.
// ---------------------------------------------------------------------------

fn selective_memo_grammar() -> (Grammar, u64) {
    use crate::portable::dynamic::{register_dynamic_callback, DynamicCallback, DynamicContext};
    use crate::portable::grammar::RepetitionTag::Maybe;

    // The block dispatches on a capture set by the enclosing rule:
    // captures -> dynamic -> outcomes, so `ditem` must stay unmemoized.
    struct SuffixDispatch;
    impl DynamicCallback for SuffixDispatch {
        fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
            if ctx.get_capture_text("m").is_some() {
                Some(Atom::Str {
                    pattern: "-B".to_string(),
                })
            } else {
                Some(Atom::Str {
                    pattern: "-A".to_string(),
                })
            }
        }
        fn description(&self) -> &str {
            "suffix dispatch"
        }
    }
    let cb_id = register_dynamic_callback(Box::new(SuffixDispatch));

    let mut g = Grammar::new();
    let word_body = g.add_atom(Atom::Re {
        pattern: "[a-z]+".to_string(),
    });
    let word = g.add_atom(Atom::Named {
        name: "word".to_string(),
        atom: word_body,
    });
    let wref = g.add_atom(Atom::Entity { atom: word });

    let letter = g.add_atom(Atom::Re {
        pattern: "[a-z]".to_string(),
    });
    let mcap = g.add_atom(Atom::Capture {
        name: "m".to_string(),
        atom: letter,
    });
    let dyna = g.add_atom(Atom::Dynamic { callback_id: cb_id });
    let ditem_body = g.add_atom(Atom::Sequence {
        atoms: vec![mcap, dyna],
    });
    let ditem = g.add_atom(Atom::Named {
        name: "ditem".to_string(),
        atom: ditem_body,
    });
    let dref = g.add_atom(Atom::Entity { atom: ditem });

    let opt = g.add_atom(Atom::Repetition {
        atom: dref,
        min: 0,
        max: Some(1),
        tag: Maybe,
    });
    let pair_body = g.add_atom(Atom::Sequence {
        atoms: vec![wref, opt],
    });
    let pair = g.add_atom(Atom::Named {
        name: "pair".to_string(),
        atom: pair_body,
    });
    let pref = g.add_atom(Atom::Entity { atom: pair });
    let root = g.add_atom(Atom::Repetition {
        atom: pref,
        min: 1,
        max: None,
        tag: crate::portable::grammar::RepetitionTag::Repetition,
    });
    g.root = root;
    (g, cb_id)
}

#[test]
fn selective_memo_eligibility_analysis() {
    let (g, _cb) = selective_memo_grammar();
    let program = compile_bytecode(g).expect("VM compiles grammar");
    assert!(program.has_invoke_dynamic());
    // Rule entries: pair, word, ditem (refs call the named rules
    // directly). Memoizable: only `word` — dynamic-free and
    // capture-free. `pair`/`ditem` are tainted, `pair` transitively
    // through its call to `ditem`.
    let eligible = program
        .memoizable_rule_count()
        .expect("dynamic program derives an eligibility map");
    assert_eq!(eligible, 1, "only the word rule may be memoizable");
}

#[test]
fn differential_selective_memo() {
    let (grammar, _cb) = selective_memo_grammar();
    let program = compile_bytecode(grammar.clone()).expect("VM compiles grammar");
    assert!(program.has_invoke_dynamic());

    for input in [
        "abc-A",
        "a-B",
        "abc-Bxyz-Aq",
        "abc",
        "abc-Ax",
        "abc-A-B",
        "abc-Bxyz",
        "a-Aa-Aa-A",
        "",
    ] {
        assert_agree_compiled(&grammar, &program, input);
    }
}

fn parg_class_grammar() -> String {
    // Classes spelled the way the PARG compiler emits them: escaped
    // ranges (`%x61-7A`), one-member classes (`%x0A`), a negated class,
    // and the whole-input class (`ANY`). The three-way word/number/
    // string choice has escaped, disjoint lead bytes, so it also runs
    // the lead-byte dispatch analysis over escaped patterns.
    let item = choice(vec![
        dynamic(capture("word", re(r"[\x61-\x7a]").many1())),
        dynamic(capture("num", re(r"[\x30-\x39]").many1())),
        dynamic(capture(
            "str",
            seq(vec![
                dynamic(re(r"[\x22]")),
                dynamic(re(r"[^\x22]").many()),
                dynamic(re(r"[\x22]")),
            ]),
        )),
        dynamic(capture(
            "comment",
            seq(vec![
                dynamic(re(r"[\x23]")),
                dynamic(
                    seq(vec![
                        dynamic(re(r"[\x0a]").not_ahead()),
                        dynamic(re(r"[\x00-\u{10ffff}]")),
                    ])
                    .many(),
                ),
            ]),
        )),
    ]);
    let ws = choice(vec![dynamic(re(r"[\x20]")), dynamic(re(r"[\x0a]"))]);
    GrammarBuilder::new()
        .rule(
            "root",
            choice(vec![dynamic(ref_("ws")), dynamic(ref_("item"))]).many(),
        )
        .rule("ws", ws)
        .rule("item", item)
        .build()
        .to_json()
        .unwrap()
}

#[test]
fn differential_parg_escaped_classes() {
    let g = parg_class_grammar();
    for input in [
        "abab",
        "zyx wvu",
        "abc 123\nxyz",
        "\"q\" \"é ü\"",
        "# cmt é\nword",
        "# no newline at end ☃",
        "ab\"unterminated",
        "ABC",
        "a-b",
        "",
    ] {
        assert_agree_collapsed(&g, input);
    }
}

#[test]
fn differential_zero_width_repetition_body() {
    // A body that matches without consuming input is counted once and
    // the loop stops, on both engines (the VM looped until its timeout).
    let lookahead_only = GrammarBuilder::new()
        .rule("root", parser_dsl::str("a").lookahead().many1())
        .build()
        .to_json()
        .unwrap();
    let guard_chain = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::str("a").lookahead()),
                dynamic(parser_dsl::str("b").not_ahead()),
            ])
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    for g in [lookahead_only, guard_chain] {
        for input in ["aaa", "a", "b", ""] {
            assert_agree(&g, input);
        }
    }
}

#[test]
fn differential_dfa_routed_loops_match_the_walker() {
    // rs#574 slice B stage 2: the unlimited plain repetitions with a
    // single single-value terminal body lower to one DfaRun (the loop
    // table's iteration-accept states synthesize the per-iteration
    // spans and the envelope). The routed VM trees must equal the
    // walker reference on every input class: full runs, partial
    // trailing iterations (the no-partial-iteration rewind), empty
    // matches (min=0), min-enforcement failures, and refusals after
    // the loop.
    let str_loop = GrammarBuilder::new()
        .rule("root", parser_dsl::str("ab").many())
        .build()
        .to_json()
        .unwrap();
    let str_loop1 = GrammarBuilder::new()
        .rule("root", parser_dsl::str("ab").many1())
        .build()
        .to_json()
        .unwrap();
    let re_loop = GrammarBuilder::new()
        .rule("root", re("[0-9]").many())
        .build()
        .to_json()
        .unwrap();
    let re_loop_in_seq = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::re("[0-9]").many()),
                dynamic(parser_dsl::str("!")),
            ]),
        )
        .build()
        .to_json()
        .unwrap();
    // The differential is only meaningful when the routing actually
    // engaged: every one of these grammars must have lowered at least
    // one loop to a DfaRun table. Without this assertion a silent
    // build_loop_dfa None would make every case below pass trivially
    // (both sides riding the BuildRep path).
    for g in [&str_loop, &str_loop1, &re_loop, &re_loop_in_seq] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "the loop in this grammar did not route to a DfaRun loop table"
        );
    }
    let str_inputs = [
        "",
        "a",
        "ab",
        "abab",
        "ababx",
        "ababab!",
        "0123456789",
        "0123!",
        "x!",
        "!",
        "aba",
        "abb",
    ];
    for input in &str_inputs {
        assert_agree_collapsed(&str_loop, input);
        assert_agree_collapsed(&str_loop1, input);
    }
    // The class-body cases run the FULL input matrix: the pre-existing
    // zero-match cross-engine divergence (ruby "" vs the rs tagged
    // empty array) is FIXED — every engine delivers the bare
    // zero-length span at the loop's position (rs#574, the ruby
    // contract), so the agreement domain covers zero-match class
    // loops too. The zero-match shapes themselves are pinned by
    // `zero_match_loops_deliver_the_ruby_empty_string`.
    for input in [
        "",
        "a",
        "ab",
        "abab",
        "ababx",
        "ababab!",
        "0123456789",
        "0123",
        "0123!",
        "x!",
        "!",
        "aba",
        "abb",
    ] {
        assert_agree_collapsed(&re_loop, input);
    }
    // The seq-embedded class loop's full matrix too: the zero-match
    // inner delivers the zero-length span the sequence fold crosses
    // (ruby joins the "" member) — both engines fold identically.
    for input in [
        "",
        "a",
        "ab",
        "abab",
        "ababx",
        "ababab!",
        "0123456789",
        "0123",
        "0123!",
        "x!",
        "!",
        "aba",
        "abb",
    ] {
        assert_agree_collapsed(&re_loop_in_seq, input);
    }
}

#[test]
fn zero_match_loops_deliver_the_ruby_empty_string() {
    // rs#574, the owner's contract decision (a): every zero-match min=0
    // repetition carries ruby's marker — the TAGGED EMPTY array in the
    // raw tree — and the flatten layers apply ruby's
    // flatten_repetition named law off it: the plain empty string ""
    // unnamed (the owner's probe matrix, ruby 1.5.13), [] at a Named
    // wrapper's direct value (the tgpp `(*part) as parts` corpus
    // shape). Pinned in the walker, the classic VM (BuildRep), and the
    // routed DfaRun lane alike.
    let class_loop = GrammarBuilder::new()
        .rule("root", re("[0-9]").many())
        .build()
        .to_json()
        .unwrap();
    let str_loop = GrammarBuilder::new()
        .rule("root", dynamic(parser_dsl::str("ab")).many())
        .build()
        .to_json()
        .unwrap();
    let body_named_loop = GrammarBuilder::new()
        .rule("root", dynamic(parser_dsl::str("a").label("r")).many())
        .build()
        .to_json()
        .unwrap();
    // A Named-WRAPPED loop (inside a sequence, the tgpp
    // `parts = (*part) as parts` shape): ruby's named law keeps the
    // empty list ([]), it never joins to "".
    let named_wrapped_loop = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![dynamic(parser_dsl::str("a").many().label("parts"))]),
        )
        .build()
        .to_json()
        .unwrap();
    // A choice body never routes: this fixture pins the CLASSIC
    // BuildRep lane. If routing ever learns choice bodies, this
    // assertion flags the fixture for a lane re-check.
    // A capture-wrapped body never routes (the capture machinery rides
    // the classic loop) — this fixture keeps pinning the classic
    // BuildRep lane. The choice-body fixture that originally held this
    // slot began routing in tranche 8.
    let alt_loop = GrammarBuilder::new()
        .rule("root", capture("x", dynamic(parser_dsl::str("a"))).many())
        .build()
        .to_json()
        .unwrap();
    for (g, routes) in [
        (&class_loop, true),
        (&str_loop, true),
        (&body_named_loop, true),
        (&alt_loop, false),
    ] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        assert_eq!(
            program.has_iteration_accept_tables(),
            routes,
            "unexpected routing lane for this fixture"
        );
        let fixture = if std::ptr::eq(g, &class_loop) {
            "class_loop"
        } else if std::ptr::eq(g, &str_loop) {
            "str_loop"
        } else if std::ptr::eq(g, &body_named_loop) {
            "body_named_loop"
        } else {
            "alt_loop"
        };
        for input in ["", "x!", "zz"] {
            let mut w_arena = AstArena::new();
            let w = PortableParser::new(&grammar, input, &mut w_arena)
                .parse_with_end_pos()
                .expect("the walker parses a zero-match loop");
            assert_eq!(w.end_pos, 0, "zero-match end_pos, {fixture} {input:?}");
            assert!(
                matches!(
                    &w.value,
                    crate::portable::ast::AstNode::Array { length: 1, .. }
                ),
                "walker raw zero-match marker (the tagged empty array), {fixture} {input:?}: {}",
                materialize(&w.value, &w_arena, input)
            );
            assert_eq!(
                materialize(&collapse_ast(&w.value, &mut w_arena), &w_arena, input),
                "s:\"\";",
                "walker collapsed zero-match wire (ruby's plain empty string), {fixture} {input:?}"
            );

            let mut v_arena = AstArena::new();
            let v = parse_with_vm(&program, input, &mut v_arena)
                .expect("the VM parses a zero-match loop");
            assert_eq!(v.end_pos, 0, "zero-match end_pos, {fixture} {input:?}");
            assert!(
                matches!(
                    &v.value,
                    crate::portable::ast::AstNode::Array { length: 1, .. }
                ),
                "VM raw zero-match marker (the tagged empty array), {fixture} {input:?}: {}",
                materialize(&v.value, &v_arena, input)
            );
            assert_eq!(
                materialize(&collapse_ast(&v.value, &mut v_arena), &v_arena, input),
                "s:\"\";",
                "VM collapsed zero-match wire (ruby's plain empty string), {fixture} {input:?}"
            );
        }
    }

    // The named law at the shape lane (to_parslet_compatible, the parg
    // corpus surface): the Named-wrapped zero-match loop keeps the
    // empty list — ruby's `return [] if named && list.empty?` — while
    // the unnamed root joins to "".
    {
        let grammar = Grammar::from_json(&named_wrapped_loop).unwrap();
        let mut arena = AstArena::new();
        let w = PortableParser::new(&grammar, "", &mut arena)
            .parse_with_end_pos()
            .expect("the walker parses a zero-match loop");
        let shaped =
            crate::portable::parslet_transform::to_parslet_compatible(&w.value, &mut arena, "");
        let shaped_text = materialize(&shaped, &arena, "");
        assert!(
            shaped_text.contains("\"parts\"->[0()]"),
            "the Named-wrapped zero-match loop keeps the empty list (ruby []): {shaped_text}"
        );
        assert!(
            !shaped_text.contains("s:\"\""),
            "the Named-wrapped zero-match loop never joins to \"\": {shaped_text}"
        );
        let grammar = Grammar::from_json(&str_loop).unwrap();
        let mut arena = AstArena::new();
        let w = PortableParser::new(&grammar, "", &mut arena)
            .parse_with_end_pos()
            .expect("the walker parses a zero-match loop");
        let shaped =
            crate::portable::parslet_transform::to_parslet_compatible(&w.value, &mut arena, "");
        assert_eq!(
            materialize(&shaped, &arena, ""),
            "s:\"\";",
            "the unnamed zero-match loop joins to the plain empty string"
        );
    }

    // The fold crosses a zero-match member: a MID-position zero-match
    // inner (the "a!" sequence) joins into one span — ruby merges the
    // "" member the same way. (The raw tree carries the tagged empty
    // marker; collapse folds it as a zero-length gap.)
    let mid_seq = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::str("a")),
                dynamic(re("[0-9]").many()),
                dynamic(parser_dsl::str("!")),
            ]),
        )
        .build()
        .to_json()
        .unwrap();
    for input in ["a!", "a1!"] {
        assert_agree_collapsed(&mid_seq, input);
    }
    {
        let grammar = Grammar::from_json(&mid_seq).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        let mut arena = AstArena::new();
        let v = parse_with_vm(&program, "a!", &mut arena).unwrap();
        let collapsed = collapse_ast(&v.value, &mut arena);
        assert_eq!(
            materialize(&collapsed, &arena, "a!"),
            "in:\"a!\";",
            "the zero-match member folds away into one span"
        );
    }

    // min=1 still raises on zero matches (the probe's RAISE row).
    let str_loop1 = GrammarBuilder::new()
        .rule("root", dynamic(parser_dsl::str("ab")).many1())
        .build()
        .to_json()
        .unwrap();
    for input in ["", "x!"] {
        let grammar = Grammar::from_json(&str_loop1).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        let mut w_arena = AstArena::new();
        assert!(
            PortableParser::new(&grammar, input, &mut w_arena)
                .parse_with_end_pos()
                .is_err(),
            "min=1 raises on a zero match, input {input:?}"
        );
        let mut v_arena = AstArena::new();
        assert!(
            parse_with_vm(&program, input, &mut v_arena).is_err(),
            "min=1 raises on a zero match, input {input:?}"
        );
    }
}

#[test]
fn differential_dfa_routed_sequence_loops_match_the_walker() {
    // rs#574 slice B stage 3: a FLAT Sequence of value-1 span
    // terminals routes to one DfaRun whose per-iteration value is the
    // `:sequence`-tagged array wrapping the merged iteration span —
    // the exact shape the classic path's per-iteration BuildSeq fold
    // produces. Raw-tree identity (not just collapsed): the synthesis
    // merges by construction the way BuildSeq's fold merges.
    let seq_str = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::str("a")),
                dynamic(parser_dsl::str("b")),
            ])
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    let seq_str1 = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::str("a")),
                dynamic(parser_dsl::str("b")),
            ])
            .many1(),
        )
        .build()
        .to_json()
        .unwrap();
    let seq_mixed = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::re("[0-9]")),
                dynamic(parser_dsl::str("x")),
            ])
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    let seq_triple = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::str("<")),
                dynamic(parser_dsl::re("[a-z]")),
                dynamic(parser_dsl::str(">")),
            ])
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    for g in [&seq_str, &seq_str1, &seq_mixed, &seq_triple] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "the sequence loop did not route to a DfaRun loop table"
        );
    }
    for input in [
        "",
        "ab",
        "abab",
        "ababx",
        "aba",
        "abb",
        "1x2x3x",
        "1x!",
        "x!",
        "<a><b><c>",
        "<a><b",
        "<>",
    ] {
        assert_agree_collapsed(&seq_str, input);
        assert_agree_collapsed(&seq_str1, input);
        assert_agree_collapsed(&seq_mixed, input);
        assert_agree_collapsed(&seq_triple, input);
    }
    // Pin the routed synthesis's raw shape: one tagged :sequence array
    // per iteration wrapping the MERGED iteration span — the classic
    // path's BuildSeq fold result. (The walker's raw tier splits mixed
    // literal+span values; the collapsed wire is the cross-engine
    // contract, per this file's header.)
    let grammar = Grammar::from_json(&seq_mixed).unwrap();
    let program = compile_bytecode(grammar).unwrap();
    let mut a = AstArena::new();
    let v = parse_with_vm(&program, "1x2x", &mut a).unwrap();
    assert_eq!(
        materialize(&v.value, &a, "1x2x"),
        "[3(s:\":repetition\";[3(s:\":sequence\";in:\"1\";in:\"x\";)];[3(s:\":sequence\";in:\"2\";in:\"x\";)];)];",
        "the routed sequence loop's per-iteration value shape"
    );
}

#[test]
fn differential_dfa_routed_named_loops_match_the_walker() {
    // rs#574 slice B stage 3 tranche 2: a Named-wrapped span terminal
    // (or flat span sequence) routes with the per-iteration value
    // `{name: inner}` — the classic BuildHash shape. Capture/Scope/
    // Trivia wrappers stay on the classic loop (capture machinery).
    let named_str = GrammarBuilder::new()
        .rule("root", dynamic(parser_dsl::str("a").label("sub")).many())
        .build()
        .to_json()
        .unwrap();
    let named_seq = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(parser_dsl::str("a")),
                    dynamic(parser_dsl::str("b")),
                ])
                .label("pair"),
            )
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    let named_mixed = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(parser_dsl::re("[0-9]")),
                    dynamic(parser_dsl::str("x")),
                ])
                .label("item"),
            )
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    let named_str1 = GrammarBuilder::new()
        .rule("root", dynamic(parser_dsl::str("a").label("sub")).many1())
        .build()
        .to_json()
        .unwrap();
    for g in [&named_str, &named_seq, &named_mixed, &named_str1] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "the named loop did not route to a DfaRun loop table"
        );
    }
    for input in [
        "", "a", "aa", "aab", "abab", "abx", "1x2x", "1x!", "<>", "<a><b>", "<a>", "abc",
    ] {
        assert_agree_collapsed(&named_str, input);
        assert_agree_collapsed(&named_seq, input);
        assert_agree_collapsed(&named_mixed, input);
        assert_agree_collapsed(&named_str1, input);
    }
    // Pin the routed synthesis's raw shape: one hash per iteration
    // wrapping the (merged) inner span.
    let grammar = Grammar::from_json(&named_mixed).unwrap();
    let program = compile_bytecode(grammar).unwrap();
    let mut a = AstArena::new();
    let v = parse_with_vm(&program, "1x2x", &mut a).unwrap();
    assert_eq!(
        materialize(&v.value, &a, "1x2x"),
        "[3(s:\":repetition\";{\"item\"->[3(s:\":sequence\";in:\"1\";in:\"x\";)];};{\"item\"->[3(s:\":sequence\";in:\"2\";in:\"x\";)];};)];",
        "the routed named loop's per-iteration value shape"
    );
}

#[test]
fn differential_dfa_routed_rule_ref_loops_match_the_walker() {
    // rs#574 slice B stage 3 tranche 3: a RULE REFERENCE as the loop
    // body routes — the classic path lowers it as a subroutine call
    // to the target's compile, so the per-iteration value = the
    // target's shape: a Named-wrapped rule hashes ({name: inner}, the
    // v2 named-tree law); an unwrapped DSL-style rule body delivers
    // bare spans / seq-tagged spans directly.
    let ref_named = GrammarBuilder::new()
        .rule("root", dynamic(ref_("sub")).many())
        .rule("sub", parser_dsl::str("a"))
        .build()
        .to_json()
        .unwrap();
    let ref_seq = GrammarBuilder::new()
        .rule("root", dynamic(ref_("sub")).many())
        .rule(
            "sub",
            seq(vec![
                dynamic(parser_dsl::str("a")),
                dynamic(parser_dsl::str("b")),
            ]),
        )
        .build()
        .to_json()
        .unwrap();
    let ref_named_label = GrammarBuilder::new()
        .rule("root", dynamic(ref_("sub")).many())
        .rule("sub", parser_dsl::str("a").label("tag"))
        .build()
        .to_json()
        .unwrap();
    let ref_terminal = GrammarBuilder::new()
        .rule("root", dynamic(ref_("sub")).many())
        .rule("sub", parser_dsl::re("[0-9]"))
        .build()
        .to_json()
        .unwrap();
    for g in [&ref_named, &ref_seq, &ref_terminal] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "the rule-ref loop did not route to a DfaRun loop table"
        );
    }
    // A DOUBLE-wrapped target (rule Named -> label Named) refuses the
    // routing for now — its per-iteration value nests hashes; it rides
    // the classic loop and joins the collapsed comparisons only.
    for input in [
        "", "a", "aa", "abab", "abx", "123", "12!", "1a2b", "xyz", "",
    ] {
        assert_agree_collapsed(&ref_named, input);
        assert_agree_collapsed(&ref_seq, input);
        assert_agree_collapsed(&ref_named_label, input);
        assert_agree_collapsed(&ref_terminal, input);
    }
    // Pin the routed synthesis's raw shapes: an unwrapped DSL-style
    // target delivers bare spans; a seq target delivers the tagged
    // array; a Named target hashes.
    let grammar = Grammar::from_json(&ref_named).unwrap();
    let program = compile_bytecode(grammar).unwrap();
    let mut a = AstArena::new();
    let v = parse_with_vm(&program, "aaa", &mut a).unwrap();
    assert_eq!(
        materialize(&v.value, &a, "aaa"),
        "[4(s:\":repetition\";in:\"a\";in:\"a\";in:\"a\";)];",
        "the unwrapped rule-ref loop's per-iteration value shape"
    );
    let grammar = Grammar::from_json(&ref_seq).unwrap();
    let program = compile_bytecode(grammar).unwrap();
    let mut a = AstArena::new();
    let v = parse_with_vm(&program, "abab", &mut a).unwrap();
    let got = materialize(&v.value, &a, "abab");
    let want =
        "[3(s:\":repetition\";[2(s:\":sequence\";in:\"ab\";)];[2(s:\":sequence\";in:\"ab\";)];)];";
    assert_eq!(
        got, want,
        "the seq-target rule-ref loop's per-iteration value shape"
    );
}

#[test]
fn scratch_dbg_route() {
    let g = GrammarBuilder::new()
        .rule("root", dynamic(ref_("sub")).many())
        .rule("sub", parser_dsl::str("a"))
        .build()
        .to_json()
        .unwrap();
    let grammar = Grammar::from_json(&g).unwrap();
    for (i, atom) in grammar.atoms.iter().enumerate() {
        eprintln!("atom[{i}] = {atom:?}");
    }
    for (name, g) in [
        (
            "ref_named",
            GrammarBuilder::new()
                .rule("sub", parser_dsl::str("a"))
                .rule("root", dynamic(ref_("sub")).many())
                .build()
                .to_json()
                .unwrap(),
        ),
        (
            "ref_seq",
            GrammarBuilder::new()
                .rule(
                    "sub",
                    seq(vec![
                        dynamic(parser_dsl::str("a")),
                        dynamic(parser_dsl::str("b")),
                    ]),
                )
                .rule("root", dynamic(ref_("sub")).many())
                .build()
                .to_json()
                .unwrap(),
        ),
        (
            "ref_terminal",
            GrammarBuilder::new()
                .rule("sub", parser_dsl::re("[0-9]"))
                .rule("root", dynamic(ref_("sub")).many())
                .build()
                .to_json()
                .unwrap(),
        ),
    ] {
        let grammar = Grammar::from_json(&g).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        eprintln!("{name} routed={}", program.has_iteration_accept_tables());
    }
}

#[test]
fn differential_dfa_routed_seq_of_refs_match_the_walker() {
    // rs#574 slice B stage 3 tranche 4: a flat Sequence whose atoms
    // are RULE REFS routes with the cut-assembly synthesis — the
    // per-iteration value = tagged `:sequence` of per-element values,
    // each cut from its element-accept crossing. Every element
    // delivers its own span (the classic fold never merges — the
    // tranche-5 characterization; the earlier merge claim here was
    // this synthesis compared against itself).
    let seq_refs = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(seq(vec![dynamic(ref_("sep")), dynamic(ref_("word"))]).many()),
        )
        .rule("sep", parser_dsl::str(" "))
        .rule("word", parser_dsl::str("ab"))
        .build()
        .to_json()
        .unwrap();
    let seq_refs1 = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(seq(vec![dynamic(ref_("sep")), dynamic(ref_("word"))]).many1()),
        )
        .rule("sep", parser_dsl::str(" "))
        .rule("word", parser_dsl::str("ab"))
        .build()
        .to_json()
        .unwrap();
    let seq_mixed_refs = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(parser_dsl::re("[0-9]")),
                    dynamic(ref_("word")),
                ])
                .many(),
            ),
        )
        .rule("word", parser_dsl::str("ab"))
        .build()
        .to_json()
        .unwrap();
    for g in [&seq_refs, &seq_refs1, &seq_mixed_refs] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "the seq-of-refs loop did not route to a DfaRun loop table"
        );
    }
    for input in [
        " ab", " ab ab", " ab abx", " abab", " ab ab ", "  ab", "ab ab", "9ab9ab", " ab9", "9",
    ] {
        assert_agree_collapsed(&seq_refs, input);
        assert_agree_collapsed(&seq_refs1, input);
        assert_agree_collapsed(&seq_mixed_refs, input);
    }
    // Pin the routed cut-assembly raw shape: every element stays its
    // OWN span (the classic fold never merges — tranche-5), for
    // inline and ref elements alike.
    let grammar = Grammar::from_json(&seq_refs).unwrap();
    let program = compile_bytecode(grammar).unwrap();
    let mut a = AstArena::new();
    let v = parse_with_vm(&program, " ab ab", &mut a).unwrap();
    assert_eq!(
        materialize(&v.value, &a, " ab ab"),
        "[3(s:\":repetition\";[3(s:\":sequence\";in:\" \";in:\"ab\";)];[3(s:\":sequence\";in:\" \";in:\"ab\";)];)];",
        "the routed seq-of-refs loop's per-iteration value shape"
    );
}

#[test]
fn differential_fold_merge_characterization() {
    // rs#574 slice B tranche 5: the fold's merge rule, CHARACTERIZED
    // against the classic engine (8983451) — the classic BuildSeq
    // NEVER merges multi-element spans: str_str (builder-fused to a
    // single Str — one element), str_re, re_str all deliver
    // per-element spans raw, walker == VM on the classic. The routed
    // cut assembly reproduces that: NO merging — every element its
    // own span. This test pins walker == VM RAW (not just collapsed)
    // per producing-instruction pair.
    let cases: Vec<(&str, parser_dsl::Sequence<parser_dsl::ErasedParslet>)> = vec![
        (
            "str_str",
            seq(vec![
                dynamic(parser_dsl::str("ab")),
                dynamic(parser_dsl::str("c")),
            ]),
        ),
        (
            "str_re",
            seq(vec![
                dynamic(parser_dsl::str("ab")),
                dynamic(parser_dsl::re("[cd]")),
            ]),
        ),
        (
            "re_str",
            seq(vec![
                dynamic(parser_dsl::re("[ab]")),
                dynamic(parser_dsl::str("c")),
            ]),
        ),
    ];
    for (name, body) in cases {
        let g = GrammarBuilder::new()
            .rule("root", dynamic(body).many())
            .build()
            .to_json()
            .unwrap();
        let grammar = Grammar::from_json(&g).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "{name}: the loop did not route"
        );
        let input = match name {
            "str_str" => "abcabc",
            "str_re" => "abdabd",
            "re_str" => "acac",
            _ => "",
        };
        let mut vm_arena = AstArena::new();
        let v = parse_with_vm(&program, input, &mut vm_arena)
            .unwrap_or_else(|e| panic!("{name}: vm failed: {e}"));
        let mut pk_arena = AstArena::new();
        let p = PortableParser::new(&grammar, input, &mut pk_arena)
            .parse_with_end_pos()
            .unwrap_or_else(|e| panic!("{name}: walker failed: {e:?}"));
        // RAW equality: the routed synthesis must produce the exact
        // classic raw tree (no merging).
        assert_eq!(
            materialize(&v.value, &vm_arena, input),
            materialize(&p.value, &pk_arena, input),
            "{name}: the routed raw tree diverges from the walker"
        );
    }
}

#[test]
fn differential_dfa_routed_nested_fragments_match_the_walker() {
    // rs#574 slice B stage 3 tranche 6: loop elements that are
    // THEMSELVES sequences route with the plan TREE — the classic
    // emits one `:sequence` node per Sequence atom (even a
    // builder-fused one-element inner), Named wraps compose as
    // `{name -> nested node}`, and refs to span-terminal rules
    // deliver bare spans inside the nested node (the classic probe
    // matrix, walker-identical on every shape).
    let nested_bare = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(seq(vec![
                        dynamic(parser_dsl::re("[ab]")),
                        dynamic(parser_dsl::str("c")),
                    ])),
                    dynamic(parser_dsl::str("d")),
                ])
                .many(),
            ),
        )
        .build()
        .to_json()
        .unwrap();
    let nested_named = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(
                        seq(vec![
                            dynamic(parser_dsl::re("[ab]")),
                            dynamic(parser_dsl::str("c")),
                        ])
                        .label("inner"),
                    ),
                    dynamic(parser_dsl::str("d")),
                ])
                .many(),
            ),
        )
        .build()
        .to_json()
        .unwrap();
    let nested_mixed = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(seq(vec![
                        dynamic(parser_dsl::re("[ab]")),
                        dynamic(parser_dsl::str("c").label("t")),
                    ])),
                    dynamic(parser_dsl::str("d")),
                ])
                .many(),
            ),
        )
        .build()
        .to_json()
        .unwrap();
    let nested_refs = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(seq(vec![dynamic(ref_("w")), dynamic(ref_("t"))])),
                    dynamic(parser_dsl::str("d")),
                ])
                .many(),
            ),
        )
        .rule("w", parser_dsl::re("[ab]"))
        .rule("t", parser_dsl::str("c"))
        .build()
        .to_json()
        .unwrap();
    let ref_seq_rule_body = GrammarBuilder::new()
        .rule("root", dynamic(ref_("pair")).many())
        .rule(
            "pair",
            seq(vec![
                dynamic(parser_dsl::re("[ab]")),
                dynamic(parser_dsl::str("c")),
            ]),
        )
        .build()
        .to_json()
        .unwrap();
    for (name, g) in [
        ("nested_bare", &nested_bare),
        ("nested_named", &nested_named),
        ("nested_mixed", &nested_mixed),
        ("nested_refs", &nested_refs),
        ("ref_seq_rule_body", &ref_seq_rule_body),
    ] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "{name}: the nested loop did not route to a DfaRun loop table"
        );
    }
    for input in [
        "acdbcd", "acd", "", "acdx", "acdbcdx", "d", "acdc", "acd acd",
    ] {
        assert_agree_collapsed(&nested_bare, input);
        assert_agree_collapsed(&nested_named, input);
        assert_agree_collapsed(&nested_mixed, input);
        assert_agree_collapsed(&nested_refs, input);
        assert_agree_collapsed(&ref_seq_rule_body, input);
    }
    // RAW equality across the whole battery: the routed plan-tree
    // synthesis must produce the exact walker raw tree (nested
    // `:sequence` nodes, hash wraps, per-leaf cuts).
    for (name, g) in [
        ("nested_bare", &nested_bare),
        ("nested_named", &nested_named),
        ("nested_mixed", &nested_mixed),
        ("nested_refs", &nested_refs),
        ("ref_seq_rule_body", &ref_seq_rule_body),
    ] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        for input in ["acdbcd", "acd", "acdx", "acdbcdx", "acd acd"] {
            let mut vm_arena = AstArena::new();
            let v = parse_with_vm(&program, input, &mut vm_arena)
                .unwrap_or_else(|e| panic!("{name} {input}: vm failed: {e}"));
            let mut pk_arena = AstArena::new();
            let p = PortableParser::new(&grammar, input, &mut pk_arena)
                .parse_with_end_pos()
                .unwrap_or_else(|e| panic!("{name} {input}: walker failed: {e:?}"));
            assert_eq!(
                materialize(&v.value, &vm_arena, input),
                materialize(&p.value, &pk_arena, input),
                "{name} {input}: the routed raw tree diverges from the walker"
            );
        }
    }
    // Pin the routed nested raw shape: the inner sequence emits its
    // own `:sequence` node; every leaf its own span.
    let grammar = Grammar::from_json(&nested_bare).unwrap();
    let program = compile_bytecode(grammar).unwrap();
    let mut a = AstArena::new();
    let v = parse_with_vm(&program, "acdbcd", &mut a).unwrap();
    assert_eq!(
        materialize(&v.value, &a, "acdbcd"),
        "[3(s:\":repetition\";[3(s:\":sequence\";[3(s:\":sequence\";in:\"a\";in:\"c\";)];in:\"d\";)];[3(s:\":sequence\";[3(s:\":sequence\";in:\"b\";in:\"c\";)];in:\"d\";)];)];",
        "the routed nested loop's per-iteration value shape"
    );
}

#[test]
fn differential_dfa_routed_loop_composition_envelopes_match_the_walker() {
    // rs#574 slice B stage 3 tranche 7: loop bodies whose elements are
    // THEMSELVES loops route with the ENVELOPE synthesis — the
    // element's value is one `:repetition` node of per-inner-iteration
    // spans (the classic's inner boundaries, recovered from the
    // inner-accept crossings). Named-wrapped inner loops compose as
    // `{inner -> [:repetition, ...]}`.
    let star_inner = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(parser_dsl::re("[ab]").many()),
                    dynamic(parser_dsl::str("c")),
                ])
                .many(),
            ),
        )
        .build()
        .to_json()
        .unwrap();
    let named_inner = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(parser_dsl::re("[ab]").many().label("inner")),
                    dynamic(parser_dsl::str("c")),
                ])
                .many(),
            ),
        )
        .build()
        .to_json()
        .unwrap();
    let bounded_inner = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(parser_dsl::re("[ab]").repeat(2, Some(2))),
                    dynamic(parser_dsl::str("c")),
                ])
                .many(),
            ),
        )
        .build()
        .to_json()
        .unwrap();
    let mixed_seq = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(parser_dsl::re("[ab]").many()),
                    dynamic(parser_dsl::str("c")),
                    dynamic(parser_dsl::re("[de]")),
                ])
                .many(),
            ),
        )
        .build()
        .to_json()
        .unwrap();
    for (name, g) in [
        ("star_inner", &star_inner),
        ("named_inner", &named_inner),
        ("bounded_inner", &bounded_inner),
        ("mixed_seq", &mixed_seq),
    ] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "{name}: the loop-composition loop did not route"
        );
    }
    for input in [
        "aacabbc",
        "aac",
        "c",
        "cabc",
        "aacaabbcabbc",
        "aacx",
        "aacabbcx",
        "aabbc",
        "",
    ] {
        assert_agree_collapsed(&star_inner, input);
        assert_agree_collapsed(&named_inner, input);
        assert_agree_collapsed(&bounded_inner, input);
        assert_agree_collapsed(&mixed_seq, input);
    }
    // RAW equality across the battery: the envelope synthesis must
    // produce the exact walker raw tree.
    for (name, g) in [
        ("star_inner", &star_inner),
        ("named_inner", &named_inner),
        ("bounded_inner", &bounded_inner),
        ("mixed_seq", &mixed_seq),
    ] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        for input in ["aacabbc", "aac", "cabc", "aacaabbcabbc", "aacabbcx"] {
            let mut vm_arena = AstArena::new();
            let v = parse_with_vm(&program, input, &mut vm_arena)
                .unwrap_or_else(|e| panic!("{name} {input}: vm failed: {e}"));
            let mut pk_arena = AstArena::new();
            let p = PortableParser::new(&grammar, input, &mut pk_arena)
                .parse_with_end_pos()
                .unwrap_or_else(|e| panic!("{name} {input}: walker failed: {e:?}"));
            assert_eq!(
                materialize(&v.value, &vm_arena, input),
                materialize(&p.value, &pk_arena, input),
                "{name} {input}: the routed raw tree diverges from the walker"
            );
        }
    }
    // Pin the routed envelope raw shape: the inner loop's envelope is
    // one `:repetition` node of per-inner-iteration spans.
    let grammar = Grammar::from_json(&star_inner).unwrap();
    let program = compile_bytecode(grammar).unwrap();
    let mut a = AstArena::new();
    let v = parse_with_vm(&program, "aacabbc", &mut a).unwrap();
    assert_eq!(
        materialize(&v.value, &a, "aacabbc"),
        "[3(s:\":repetition\";[3(s:\":sequence\";[3(s:\":repetition\";in:\"a\";in:\"a\";)];in:\"c\";)];[3(s:\":sequence\";[4(s:\":repetition\";in:\"a\";in:\"b\";in:\"b\";)];in:\"c\";)];)];",
        "the routed loop-composition loop's per-iteration value shape"
    );
}

#[test]
fn differential_dfa_routed_choice_bodies_match_the_walker() {
    // rs#574 slice B stage 3 tranche 8: a loop body that is an
    // Alternative of bare span arms with pairwise-disjoint FIRST sets
    // routes — the per-iteration value is ONE bare span (the matched
    // arm), raw-identical to the classic ordered-choice loop's value.
    // Disjointness is the provability precondition: under it PEG
    // ordered-choice == DFA longest-match, so the routed table is exact.
    let choice_loop = GrammarBuilder::new()
        .rule(
            "root",
            choice(vec![
                dynamic(parser_dsl::str("a")),
                dynamic(parser_dsl::str("b")),
                dynamic(parser_dsl::str("cd")),
            ])
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    let named_choice_loop = GrammarBuilder::new()
        .rule(
            "root",
            choice(vec![
                dynamic(parser_dsl::str("a")),
                dynamic(parser_dsl::str("b")),
            ])
            .label("n")
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    // A choice ELEMENT inside a flat sequence body: one leaf slot, the
    // matched arm's span between its neighbors.
    let choice_in_seq = GrammarBuilder::new()
        .rule(
            "root",
            dynamic(
                seq(vec![
                    dynamic(parser_dsl::str("x")),
                    dynamic(choice(vec![
                        dynamic(parser_dsl::str("a")),
                        dynamic(parser_dsl::str("b")),
                    ])),
                    dynamic(parser_dsl::str("y")),
                ])
                .label("pair"),
            )
            .many(),
        )
        .build()
        .to_json()
        .unwrap();
    // Overlapping FIRSTs refuse the routing (PEG ordered-choice would
    // diverge from DFA longest-match) — the classic path keeps the
    // loop; the engagement assertion pins the refusal.
    let overlapping = GrammarBuilder::new()
        .rule(
            "root",
            choice(vec![
                dynamic(parser_dsl::str("ab")),
                dynamic(parser_dsl::str("a")),
            ])
            .many(),
        )
        .build()
        .to_json()
        .unwrap();

    for g in [&choice_loop, &named_choice_loop, &choice_in_seq] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "the choice-body loop did not route to a DfaRun loop table"
        );
    }
    {
        let grammar = Grammar::from_json(&overlapping).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        assert!(
            !program.has_iteration_accept_tables(),
            "overlapping FIRSTs must refuse the routing (PEG ordered-choice semantics)"
        );
    }

    for input in [
        "", "a", "ab", "ab2", "cd", "abcd", "cdab", "bacd", "abcdae", "xyxy", "xayxby", "xaybxy",
        "xqy", "e",
    ] {
        assert_agree_collapsed(&choice_loop, input);
        assert_agree_collapsed(&named_choice_loop, input);
        assert_agree_collapsed(&choice_in_seq, input);
        assert_agree_collapsed(&overlapping, input);
    }

    // Raw equality on the core inputs: the routed per-iteration value
    // (one bare span, the matched arm) is identical to the classic's.
    for input in ["ab", "cdab", "xaybxy", "abcdae"] {
        let grammar = Grammar::from_json(&choice_loop).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        let mut w_arena = AstArena::new();
        let w = PortableParser::new(&grammar, input, &mut w_arena)
            .parse_with_end_pos()
            .expect("the walker parses");
        let mut v_arena = AstArena::new();
        let v = parse_with_vm(&program, input, &mut v_arena).expect("the VM parses");
        assert_eq!(w.end_pos, v.end_pos, "end_pos differs for {input:?}");
        assert_eq!(
            materialize(&w.value, &w_arena, input),
            materialize(&v.value, &v_arena, input),
            "RAW tree differs for {input:?}"
        );
    }

    // The routed raw shape pin: one bare span per iteration, the
    // matched arm's text.
    {
        let grammar = Grammar::from_json(&choice_loop).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        let mut a = AstArena::new();
        let v = parse_with_vm(&program, "cdab", &mut a).unwrap();
        let shape = materialize(&v.value, &a, "cdab");
        assert!(
            shape.contains("s:\":repetition\";")
                && shape.contains("in:\"cd\";")
                && shape.contains("in:\"a\";")
                && shape.contains("in:\"b\";"),
            "the routed choice loop's per-iteration value shape: {shape}"
        );
    }
    // The named choice's per-iteration hash.
    {
        let grammar = Grammar::from_json(&named_choice_loop).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        let mut a = AstArena::new();
        let v = parse_with_vm(&program, "ab", &mut a).unwrap();
        let shape = materialize(&v.value, &a, "ab");
        assert!(
            shape.contains("s:\":repetition\";")
                && shape.contains("{\"n\"->in:\"a\";};")
                && shape.contains("{\"n\"->in:\"b\";};"),
            "the routed named choice loop's per-iteration value shape: {shape}"
        );
    }
    // The choice element's slot between its neighbors.
    {
        let grammar = Grammar::from_json(&choice_in_seq).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        let mut a = AstArena::new();
        let v = parse_with_vm(&program, "xayxby", &mut a).unwrap();
        let shape = materialize(&v.value, &a, "xayxby");
        assert!(
            shape.contains("{\"pair\"->[4(s:\":sequence\";in:\"x\";in:\"a\";in:\"y\";)];};")
                && shape.contains("{\"pair\"->[4(s:\":sequence\";in:\"x\";in:\"b\";in:\"y\";)];};"),
            "the routed choice-element slot shape: {shape}"
        );
    }

    // Tranche-8's envelope-exit catch (the Specs guard): a min>=1
    // UNBOUNDED envelope element inside a routed loop body must let the
    // outer iteration close after the mandatory copies — the arithmetic
    // tail `ws* op ws* 1*digit` (the op = a choice element) parsed "1"
    // and stopped at the first "+" before the fix: the number element
    // demanded a SECOND inner iteration before any boundary was
    // reachable. The grammar is the backend parity suite's
    // arithmetic_grammar shape.
    let arithmetic_tail = {
        let mut g = Grammar::new();
        let digit = g.add_atom(Atom::Re {
            pattern: "[0-9]".to_string(),
        });
        let number = g.add_atom(Atom::Repetition {
            atom: digit,
            min: 1,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let plus = g.add_atom(Atom::Str {
            pattern: "+".to_string(),
        });
        let minus = g.add_atom(Atom::Str {
            pattern: "-".to_string(),
        });
        let op = g.add_atom(Atom::Alternative {
            atoms: vec![plus, minus],
        });
        let ws = g.add_atom(Atom::Re {
            pattern: "[ \t]".to_string(),
        });
        let ws_opt = g.add_atom(Atom::Repetition {
            atom: ws,
            min: 0,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let op_and_num = g.add_atom(Atom::Sequence {
            atoms: vec![ws_opt, op, ws_opt, number],
        });
        g.root = op_and_num;
        g.to_json().unwrap()
    };
    {
        let grammar = Grammar::from_json(&arithmetic_tail).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "the mixed envelope/choice tail did not route"
        );
        for (input, end) in [("+2", 2usize), (" + 2 ", 4), ("+-1", 0)] {
            let mut w_arena = AstArena::new();
            let w = PortableParser::new(&grammar, input, &mut w_arena).parse_with_end_pos();
            let mut v_arena = AstArena::new();
            let v = parse_with_vm(&program, input, &mut v_arena);
            match (&w, &v) {
                (Ok(w), Ok(v)) => {
                    assert_eq!(
                        w.end_pos, v.end_pos,
                        "envelope-exit end_pos differs for {input:?}"
                    );
                    assert_eq!(
                        v.end_pos, end,
                        "envelope-exit consumed length for {input:?}"
                    );
                }
                (Err(_), Err(_)) => {}
                (w, v) => {
                    panic!("envelope-exit success/failure mismatch for {input:?}: w={w:?} v={v:?}")
                }
            }
        }
    }
}

// rs#574 slice B stage 3 tranche 9: per-arm CHOICE slots — the arms'
// leaf slots are contiguous ranges inside the arm union, and the
// assembly picks the arm whose LAST leaf slot carries this iteration's
// boundary cut. The prize shape is the string interior
// (escape / char)* — mixed arm leaf counts (2 vs 1) with per-arm slot
// ranges. The multi-byte class slice lifts the stage-1 ASCII law:
// classes with members above 0x7F lower to UTF-8 sequence chains
// (regex-syntax's canonical Utf8Sequences decomposition), so the
// JSON grammar's exact `[^"\\]` arm routes and a multi-byte codepoint
// still consumes ALL its bytes as ONE iteration value (the walker's
// match_at shape).
#[test]
fn differential_dfa_routed_per_arm_choice_slots_match_the_walker() {
    // The ASCII-bounded string interior: `"` (escape / char)* `"`,
    // where escape is a TWO-leaf sequence arm (backslash + the escaped
    // char) and char is a one-leaf class arm (printable ASCII minus
    // quote and backslash). The arms' FIRST sets are disjoint (\ vs
    // non-\), so PEG ordered-choice == DFA longest-match and the
    // routed table is exact.
    let string_interior = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::str("\"")),
                dynamic(
                    choice(vec![
                        dynamic(seq(vec![
                            dynamic(parser_dsl::str("\\")),
                            dynamic(re(r#"["\\]"#)),
                        ])),
                        dynamic(re("[a-zA-Z0-9_ ]")),
                    ])
                    .many(),
                ),
                dynamic(parser_dsl::str("\"")),
            ]),
        )
        .build()
        .to_json()
        .unwrap();
    // The JSON grammar's EXACT interior minus the u-hexquad: the char
    // arm is the negated Unicode class `[^"\\]` — multi-byte members
    // ride UTF-8 sequence chains and one `é`/emoji iteration is ONE
    // span. (The hexquad itself — `u` + min-4-unbounded hexclass — is
    // refused by the ENVELOPE-TAIL OVERLAP law, pinned below: the
    // hexclass's bytes intersect the loop restart's plain-char bytes,
    // so the subset construction would merge the greedy-tail and
    // loop-restart interpretations into one single-path state.)
    let negated_interior = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::str("\"")),
                dynamic(
                    choice(vec![
                        dynamic(seq(vec![
                            dynamic(parser_dsl::str("\\")),
                            dynamic(re(r#"["\\/bfnrt]"#)),
                        ])),
                        dynamic(re(r#"[^"\\]"#)),
                    ])
                    .many(),
                ),
                dynamic(parser_dsl::str("\"")),
            ]),
        )
        .build()
        .to_json()
        .unwrap();
    // The ENVELOPE-TAIL OVERLAP refusal pin: the escape arm's
    // min-4-unbounded hexclass (the json u-hexquad shape) overlaps the
    // loop restart's plain-char bytes — the routed lane must refuse
    // the body (the classic loop keeps it).
    let hexquad_interior = GrammarBuilder::new()
        .rule(
            "root",
            seq(vec![
                dynamic(parser_dsl::str("\"")),
                dynamic(
                    choice(vec![
                        dynamic(seq(vec![
                            dynamic(parser_dsl::str("\\")),
                            dynamic(choice(vec![
                                dynamic(re(r#"["\\/bfnrt]"#)),
                                dynamic(seq(vec![
                                    dynamic(parser_dsl::str("u")),
                                    dynamic(re("[0-9a-fA-F]").repeat(4, None)),
                                ])),
                            ])),
                        ])),
                        dynamic(re(r#"[^"\\]"#)),
                    ])
                    .many(),
                ),
                dynamic(parser_dsl::str("\"")),
            ]),
        )
        .build()
        .to_json()
        .unwrap();
    // A mixed-count choice body where the LONGER arm fires on a later
    // iteration than the shorter one — per-arm slot reuse across
    // iterations (the stale-cut pick hazard).
    let mixed = GrammarBuilder::new()
        .rule(
            "root",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(parser_dsl::str("x")),
                    dynamic(parser_dsl::str("y")),
                ])),
                dynamic(parser_dsl::str("z")),
            ])
            .many(),
        )
        .build()
        .to_json()
        .unwrap();

    for g in [&string_interior, &negated_interior, &mixed] {
        let grammar = Grammar::from_json(g).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        assert!(
            program.has_iteration_accept_tables(),
            "the per-arm choice body did not route to a DfaRun loop table"
        );
    }
    {
        let grammar = Grammar::from_json(&hexquad_interior).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        // The INTERIOR's choice loop must refuse (the overlap law);
        // the hexquad's own nested loop still routes via tranche 8's
        // bare-arm lane, so the pin is the absence of any tranche-9
        // plan tree — the interior is the grammar's only tranche-9
        // candidate.
        for idx in 0..8u32 {
            assert!(
                program.get_dfa_loop_plan_trees9(idx).is_none(),
                "the hexquad-overlap interior routed (the envelope-tail overlap law must refuse it)"
            );
        }
    }

    for input in [
        "\"\"",
        "\"a\"",
        "\"ab\"",
        "\"a\\\"b\"",
        "\"a\\\\b\"",
        "\"ab\\\"cd\\\\ef gh\"",
        "\"a\\nb\\t\\/c\"",
        "\"xzy\"",
        "\"zxz\"",
        "\"xyxy\"",
        "\"xyxzy\"",
    ] {
        for g in [&string_interior, &negated_interior] {
            let grammar = Grammar::from_json(g).unwrap();
            let program = compile_bytecode(grammar.clone()).unwrap();
            let mut w_arena = AstArena::new();
            let w = PortableParser::new(&grammar, input, &mut w_arena).parse_with_end_pos();
            let mut v_arena = AstArena::new();
            let v = parse_with_vm(&program, input, &mut v_arena);
            match (&w, &v) {
                (Ok(w), Ok(v)) => {
                    assert_eq!(
                        w.end_pos, v.end_pos,
                        "string-interior end_pos differs for {input:?}"
                    );
                    assert_eq!(
                        materialize(&w.value, &w_arena, input),
                        materialize(&v.value, &v_arena, input),
                        "string-interior tree differs for {input:?}"
                    );
                }
                (Err(_), Err(_)) => {}
                (w, v) => panic!(
                    "string-interior success/failure mismatch for {input:?}: w={w:?} v={v:?}"
                ),
            }
        }
    }
    for input in ["", "z", "xy", "xyzx", "xyxyz", "zxxy", "xyxzxy"] {
        assert_agree_collapsed(&mixed, input);
    }
    // The refused hexquad body stays EXACT on the classic path —
    // including the greedy tail swallowing following hex digits.
    for input in [
        "\"\"",
        "\"a\\u0041b\"",
        "\"\\u00e9x\"",
        "\"a\\n\\u0041\\/b\"",
    ] {
        assert_agree_collapsed(&hexquad_interior, input);
    }

    // The multi-byte members: one iteration per CODEPOINT — `é` (2
    // bytes) and an astral emoji (4 bytes) are single spans between
    // the ASCII chars, and a truncated sequence mid-string is no
    // member at all (both engines reject).
    for input in [
        "\"café\"",
        "\"naïve\"",
        "\"a\u{1F600}b\"",
        "\"e\u{0301}\"",
        "\"xy\u{E9}",
        "\"\u{4e2d}\u{6587}\"",
    ] {
        let grammar = Grammar::from_json(&negated_interior).unwrap();
        let program = compile_bytecode(grammar.clone()).unwrap();
        let mut w_arena = AstArena::new();
        let w = PortableParser::new(&grammar, input, &mut w_arena).parse_with_end_pos();
        let mut v_arena = AstArena::new();
        let v = parse_with_vm(&program, input, &mut v_arena);
        match (&w, &v) {
            (Ok(w), Ok(v)) => {
                assert_eq!(
                    w.end_pos, v.end_pos,
                    "multi-byte end_pos differs for {input:?}"
                );
                assert_eq!(
                    materialize(&w.value, &w_arena, input),
                    materialize(&v.value, &v_arena, input),
                    "multi-byte tree differs for {input:?}"
                );
            }
            (Err(_), Err(_)) => {}
            (w, v) => {
                panic!("multi-byte success/failure mismatch for {input:?}: w={w:?} v={v:?}")
            }
        }
    }
    // The per-iteration VALUE for a multi-byte char is ONE span.
    {
        let grammar = Grammar::from_json(&negated_interior).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        let mut a = AstArena::new();
        let input = "\"xy\u{E9}\"";
        let v = parse_with_vm(&program, input, &mut a).unwrap();
        let shape = materialize(&v.value, &a, input);
        assert!(
            shape.contains("in:\"x\";")
                && shape.contains("in:\"y\";")
                && shape.contains("in:\"\u{E9}\";"),
            "the multi-byte iteration value is one whole-codepoint span: {shape}"
        );
    }

    // The per-arm slot shape pin: each interior char is ONE bare span
    // per iteration — the two-leaf escape arm contributes its WHOLE
    // `\x` span (the sequence arm's merged leaves), the class arm its
    // single char.
    {
        let grammar = Grammar::from_json(&string_interior).unwrap();
        let program = compile_bytecode(grammar).unwrap();
        let mut a = AstArena::new();
        let v = parse_with_vm(&program, "\"a\\\"b\"", &mut a).unwrap();
        let shape = materialize(&v.value, &a, "\"a\\\"b\"");
        assert!(
            shape.contains("in:\"\\\"\";")
                && shape.contains("in:\"a\";")
                && shape.contains("in:\"\\\"\";")
                && shape.contains("in:\"b\";"),
            "the per-arm slot iteration values: {shape}"
        );
    }
}
