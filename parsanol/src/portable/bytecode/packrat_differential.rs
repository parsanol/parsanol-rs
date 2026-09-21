//! Differential tests: the bytecode VM must agree with the packrat
//! tree-walker on success/failure, consumed length, and the materialized
//! AST for every grammar in the battery. This is the gate that must hold
//! before the VM can serve any production parse path.

#![cfg(test)]

use crate::portable::arena::AstArena;
use crate::portable::bytecode::{compile_bytecode, parse_with_vm};
use crate::portable::grammar::{Atom, Grammar};
use crate::portable::parser::PortableParser;
use crate::portable::parser_dsl::{
    capture, choice, dynamic, re, ref_, seq, str, GrammarBuilder, ParsletExt,
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

// ---------------------------------------------------------------------------
// Grammar battery
// ---------------------------------------------------------------------------

fn kv_grammar() -> String {
    let value = choice(vec![
        dynamic(seq(vec![
            dynamic(ref_("ident")),
            dynamic(str(".")),
            dynamic(ref_("ident")),
        ])),
        dynamic(ref_("ident")),
        dynamic(ref_("number")),
    ]);
    let line = seq(vec![
        dynamic(ref_("ident")),
        dynamic(re(r"\s+").ignore()),
        dynamic(str("=")),
        dynamic(re(r"\s+").ignore()),
        dynamic(ref_("value")),
        dynamic(str(";")),
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
                    dynamic(str("\"")),
                    dynamic(capture("body", re(r#"[^"]*"#))),
                    dynamic(str("\"")),
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
        dynamic(str("if").lookahead()),
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
            dynamic(str("(")),
            dynamic(capture("group", ref_("list"))),
            dynamic(str(")")),
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
            dynamic(str("(")),
            dynamic(capture("group", ref_("list"))),
            dynamic(str(")")),
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
