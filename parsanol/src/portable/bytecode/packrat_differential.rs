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
    // The class-body cases ride only the inputs where every site
    // matches: a zero-match `*class` loop hits a PRE-EXISTING
    // cross-engine divergence (the shipped ruby delivers "" for a
    // zero-match loop; the rs engines keep the tagged empty array —
    // filed on rs#574 for the owner's contract decision). This
    // differential pins the routing against the walker on the
    // agreement domain.
    for input in ["0123456789", "0123"] {
        assert_agree_collapsed(&re_loop, input);
    }
    for input in ["0123!", "x!", "ababab!"] {
        assert_agree_collapsed(&re_loop_in_seq, input);
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
