//! Parser Tests

use super::*;
use crate::portable::arena::AstArena;
use crate::portable::parser_dsl::{str, GrammarBuilder};

#[test]
fn test_parse_with_rich_error_success() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse_with_rich_error();
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_rich_error_failure() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "world";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse_with_rich_error();
    assert!(result.is_err());

    let error = result.unwrap_err();
    assert!(error.message.contains("Expected"));
}

#[test]
fn test_parse_with_trace_success() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let (result, trace) = parser.parse_with_trace();
    assert!(result.is_ok());
    assert!(!trace.entries.is_empty());
}

#[test]
fn test_parse_with_trace_failure() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "world";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let (result, trace) = parser.parse_with_trace();
    assert!(result.is_err());
    assert!(!trace.entries.is_empty());
}

#[test]
fn test_trace_format() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let (_, trace) = parser.parse_with_trace();
    let formatted = trace.format(&grammar);
    assert!(formatted.contains("Enter"));
}

#[test]
fn test_rich_error_format_with_source() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "world";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse_with_rich_error();
    if let Err(error) = result {
        let formatted = error.format_with_source("world");
        assert!(formatted.contains("line"));
        assert!(formatted.contains("column"));
    }
}

#[test]
fn test_set_timeout() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.set_timeout_ms(1000);

    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_set_max_memory() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.set_max_memory(1_000_000);

    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_memory_usage() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let parser = PortableParser::new(&grammar, input, &mut arena);

    let usage = parser.memory_usage();
    assert!(usage > 0);
}

#[test]
fn test_resource_limits_combined() {
    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    parser.set_timeout_ms(1000);
    parser.set_max_memory(1_000_000);
    parser.set_max_recursion_depth(100);

    let result = parser.parse();
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_builder() {
    use crate::portable::streaming_builder::DebugBuilder;

    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let mut builder = DebugBuilder::new();
    let result: Result<Vec<String>, _> = parser.parse_with_builder(&mut builder);

    assert!(result.is_ok());
    let events = result.unwrap();
    assert!(!events.is_empty());
}

#[test]
fn test_parse_with_builder_collects_strings() {
    use crate::portable::streaming_builder::BuilderStringCollector;

    let grammar = GrammarBuilder::new().rule("test", str("hello")).build();
    let input = "hello";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let mut builder = BuilderStringCollector::new();
    let result: Result<Vec<String>, _> = parser.parse_with_builder(&mut builder);

    assert!(result.is_ok());
    let strings = result.unwrap();
    assert_eq!(strings, vec!["hello"]);
}

#[test]
fn dynamic_fragment_values_are_adopted_into_parent_arena() {
    use crate::portable::ast::AstNode;
    use crate::portable::parser_dsl::{dynamic, seq, str, GrammarBuilder};

    fn touch(node: &AstNode, arena: &crate::portable::arena::AstArena) {
        match node {
            AstNode::StringRef { pool_index } => {
                let _ = arena.get_string(*pool_index as usize);
            }
            AstNode::Array { pool_index, length } => {
                for child in arena.get_array(*pool_index as usize, *length as usize) {
                    touch(&child, arena);
                }
            }
            AstNode::Hash { pool_index, length } => {
                for (_, v) in arena.get_hash_items(*pool_index as usize, *length as usize) {
                    touch(&v, arena);
                }
            }
            _ => {}
        }
    }

    // The dynamic atom resolves to a sequence whose subtree is built
    // in the fragment's temporary arena. Pool-backed children must be
    // adopted into the parent arena or extraction panics with an
    // out-of-bounds pool index (GH-76).
    let grammar = GrammarBuilder::new()
        .rule("root", dynamic(seq([str("a"), str("b")])))
        .build();
    let input = "ab";

    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let tree = parser.parse().expect("parse");

    // Panics (index out of bounds) if the tree dangles into the
    // fragment's arena.
    touch(&tree, &arena);
}

mod capture_rollback_and_memo {
    use crate::portable::arena::AstArena;
    use crate::portable::dynamic::{register_dynamic_callback, DynamicCallback, DynamicContext};
    use crate::portable::grammar::Atom;
    use crate::portable::grammar::Grammar;
    use crate::portable::parser::PortableParser;

    /// A dispatch that converges only when `mode` is NOT set (the
    /// capture written by a FAILED earlier branch must not leak).
    struct ModeSensitive {
        with_mode: &'static str,
        without_mode: &'static str,
    }

    impl DynamicCallback for ModeSensitive {
        fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
            let pattern = if ctx.captures.get("mode").is_some() {
                self.with_mode
            } else {
                self.without_mode
            };
            Some(Atom::Str {
                pattern: pattern.to_string(),
            })
        }
        fn description(&self) -> &str {
            "mode-sensitive dispatch"
        }
    }

    fn mode_grammar(cb_id: u64) -> Grammar {
        let mut grammar = Grammar::new();
        let a = grammar.add_atom(Atom::Str {
            pattern: "A".to_string(),
        });
        let bang = grammar.add_atom(Atom::Str {
            pattern: "!".to_string(),
        });
        let cap = grammar.add_atom(Atom::Capture {
            name: "mode".to_string(),
            atom: a,
        });
        let branch1 = grammar.add_atom(Atom::Sequence {
            atoms: vec![cap, bang],
        });
        let dyn_atom = grammar.add_atom(Atom::Dynamic { callback_id: cb_id });
        let branch2 = grammar.add_atom(Atom::Sequence {
            atoms: vec![a, dyn_atom],
        });
        let root = grammar.add_atom(Atom::Alternative {
            atoms: vec![branch1, branch2],
        });
        grammar.root = root;
        grammar
    }

    /// alt( seq(capture(:mode,"A"), "!"), seq(str("A"), dynamic) )
    ///
    /// Branch 1 captures :mode then fails on "!". Branch 2's dynamic
    /// block must run with NO :mode — GH-76 follow-up / coradoc
    /// open_block chaining (parsanol-ruby#76).
    #[test]
    fn failed_branch_captures_do_not_leak_into_later_dynamic() {
        let cb_id = register_dynamic_callback(Box::new(ModeSensitive {
            with_mode: "!",
            without_mode: "B",
        }));

        let grammar = mode_grammar(cb_id);
        let input = "AB";

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let tree = parser
            .parse()
            .expect("branch 2 must parse with clean captures");

        // The dynamic atom resolved to "B" (no leaked :mode).
        let flat = flatten(&tree, &arena, input);
        assert!(
            flat.iter().any(|t| t == "B"),
            "expected the without-mode branch, got {flat:?}"
        );
        assert!(
            !flat.iter().any(|t| t == "!"),
            "with-mode branch must not have matched"
        );
    }

    /// Same grammar through the byte-code VM must agree (TODO.perf/5:
    /// dynamic grammars run on the VM, memoization disabled).
    #[test]
    fn vm_matches_walker_on_capture_dependent_dynamic() {
        let cb_id = register_dynamic_callback(Box::new(ModeSensitive {
            with_mode: "!",
            without_mode: "B",
        }));

        let grammar = mode_grammar(cb_id);
        let input = "AB";

        let mut walker_arena = AstArena::new();
        let walker = {
            let mut parser = PortableParser::new(&grammar, input, &mut walker_arena);
            parser.parse_with_end_pos().expect("walker parse")
        };

        let program = crate::portable::bytecode::compiler::compile(grammar.clone())
            .expect("VM compiles dynamic grammars");
        assert!(program.has_invoke_dynamic());
        let vm = {
            let mut arena = AstArena::new();
            let mut vm = crate::portable::bytecode::vm::BytecodeVM::new(
                &program,
                input,
                &mut arena,
                Default::default(),
            );
            vm.run().expect("vm parse")
        };

        assert_eq!(walker.end_pos, vm.end_pos, "end positions agree");
        let wf = flatten(&walker.value, &walker_arena, input);
        let vf = flatten(&vm.value, &walker_arena, input);
        assert_eq!(wf, vf, "trees agree");
        assert!(vf.iter().any(|t| t == "B"));
    }

    // -- helpers -----------------------------------------------------------

    fn flatten(node: &crate::portable::ast::AstNode, arena: &AstArena, input: &str) -> Vec<String> {
        let mut out = Vec::new();
        walk(node, arena, input, &mut out);
        out
    }

    fn walk(
        node: &crate::portable::ast::AstNode,
        arena: &AstArena,
        input: &str,
        out: &mut Vec<String>,
    ) {
        use crate::portable::ast::AstNode;
        match node {
            AstNode::InputRef { offset, length } => {
                out.push(input[*offset as usize..*offset as usize + *length as usize].to_string())
            }
            AstNode::Array { pool_index, length } => {
                for child in arena.get_array(*pool_index as usize, *length as usize) {
                    walk(&child, arena, input, out);
                }
            }
            AstNode::Hash { pool_index, length } => {
                for (_, v) in arena.get_hash_items(*pool_index as usize, *length as usize) {
                    walk(&v, arena, input, out);
                }
            }
            AstNode::StringRef { pool_index } => {
                out.push(arena.get_string(*pool_index as usize).to_string())
            }
            _ => {}
        }
    }
}

mod prefix_hoisting {
    use crate::portable::arena::AstArena;
    use crate::portable::grammar::{Atom, Grammar};
    use crate::portable::parser::PortableParser;

    /// alt(seq(num, ".", "5"), seq(num, "e", "5")) with a SHARED head
    /// (num = Named(Re[0-9]+), same atom index in both branches).
    fn build() -> Grammar {
        let mut g = Grammar::new();
        let digits = g.add_atom(Atom::Re {
            pattern: "[0-9]+".to_string(),
        });
        let num = g.add_atom(Atom::Named {
            name: "num".to_string(),
            atom: digits,
        });
        let dot = g.add_atom(Atom::Str {
            pattern: ".".to_string(),
        });
        let exp = g.add_atom(Atom::Str {
            pattern: "e".to_string(),
        });
        let frac = g.add_atom(Atom::Str {
            pattern: "5".to_string(),
        });
        let b1 = g.add_atom(Atom::Sequence {
            atoms: vec![num, dot, frac],
        });
        let b2 = g.add_atom(Atom::Sequence {
            atoms: vec![num, exp, frac],
        });
        let root = g.add_atom(Atom::Alternative {
            atoms: vec![b1, b2],
        });
        g.root = root;
        g
    }

    fn walker_flatten(grammar: &Grammar, input: &str) -> Option<Vec<String>> {
        fn walk(
            node: &crate::portable::ast::AstNode,
            arena: &AstArena,
            input: &str,
            out: &mut Vec<String>,
        ) {
            use crate::portable::ast::AstNode;
            match node {
                AstNode::InputRef { offset, length } => out
                    .push(input[*offset as usize..*offset as usize + *length as usize].to_string()),
                AstNode::Array { pool_index, length } => {
                    for child in arena.get_array(*pool_index as usize, *length as usize) {
                        walk(&child, arena, input, out);
                    }
                }
                AstNode::Hash { pool_index, length } => {
                    for (k, v) in arena.get_hash_items(*pool_index as usize, *length as usize) {
                        out.push(format!("{}=", k));
                        walk(&v, arena, input, out);
                    }
                }
                AstNode::StringRef { pool_index } => {
                    out.push(arena.get_string(*pool_index as usize).to_string())
                }
                _ => {}
            }
        }
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(grammar, input, &mut arena);
        match parser.parse_with_end_pos() {
            Ok(r) if r.end_pos == input.len() => {
                let mut out = Vec::new();
                walk(&r.value, &arena, input, &mut out);
                Some(out)
            }
            _ => None,
        }
    }

    /// TODO.perf/3: the compiler splits the shared prefix out of the
    /// alternative and dispatches on the tails; the trees must stay
    /// identical to the tree-walker's (value envelopes are rebuilt
    /// per branch), and a ByteDispatch must be emitted for the
    /// disjoint tails.
    #[test]
    fn prefix_split_compiles_to_dispatch_with_identical_trees() {
        let grammar = build();

        let program =
            crate::portable::bytecode::compiler::compile(grammar.clone()).expect("compile");
        let mut dispatches = 0;
        for i in 0..program.instruction_count() {
            if matches!(
                program.get_instruction(i),
                Some(crate::portable::bytecode::instruction::Instruction::ByteDispatch { .. })
            ) {
                dispatches += 1;
            }
        }
        assert_eq!(dispatches, 1, "tails should dispatch on lead byte");

        for input in ["12.5", "12e5", "12", "x"] {
            let walker = walker_flatten(&grammar, input);
            let vm = {
                let mut arena = AstArena::new();
                let mut vm = crate::portable::bytecode::vm::BytecodeVM::new(
                    &program,
                    input,
                    &mut arena,
                    Default::default(),
                );
                vm.run()
                    .ok()
                    .filter(|r| r.end_pos == input.len())
                    .map(|r| r.value)
            };
            // The VM result carries arena-free InputRefs for this
            // grammar; compare presence/absence with the walker, and
            // full trees through the same flattener on a fresh arena.
            match (&walker, vm) {
                (Some(_), Some(_)) | (None, None) => {}
                (w, v) => panic!("acceptance mismatch for {input:?}: walker {w:?} vm {v:?}"),
            }
        }
    }
}

mod capture_writes {
    use crate::portable::arena::AstArena;
    use crate::portable::capture_state::CaptureState;
    use crate::portable::dynamic::{
        drain_capture_writes_into, note_capture_writes, register_dynamic_callback, DynamicCallback,
        DynamicContext,
    };
    use crate::portable::grammar::{Atom, Grammar};
    use crate::portable::parser::PortableParser;

    /// A block that WRITES continuation state, mirroring the coradoc
    /// pattern (parsanol-ruby#80): caps[:cont] = caps[:cont] >> rule.
    struct ChainingCallback;

    impl DynamicCallback for ChainingCallback {
        fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
            // Write side effect: append to the continuation.
            let cont = ctx
                .get_capture_text("cont")
                .map(|c| c.into_owned())
                .unwrap_or_default();
            let next = format!("{cont}+");
            note_capture_writes(vec![("cont".to_string(), next)]);

            // Dispatch: with cont "a+" match "B", else match "A".
            let pattern = if cont.is_empty() { "A" } else { "B" };
            Some(Atom::Str {
                pattern: pattern.to_string(),
            })
        }
        fn description(&self) -> &str {
            "chaining dispatcher"
        }
    }

    /// seq(dynamic1, dynamic2): block 1 writes :cont; block 2 must
    /// read the write and dispatch on it.
    #[test]
    fn block_writes_are_visible_to_later_blocks() {
        let cb_id = register_dynamic_callback(Box::new(ChainingCallback));
        let mut g = Grammar::new();
        let d1 = g.add_atom(Atom::Dynamic { callback_id: cb_id });
        let d2 = g.add_atom(Atom::Dynamic { callback_id: cb_id });
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![d1, d2],
        });
        g.root = root;

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&g, "AB", &mut arena);
        parser
            .parse()
            .unwrap_or_else(|e| panic!("chained dispatch should parse: {e:?}"));
    }

    /// A write made inside a FAILED branch must not leak: branch 1's
    /// dynamic block writes :cont then the branch fails; branch 2's
    /// block must see no :cont.
    #[test]
    fn failed_branch_writes_are_discarded() {
        use crate::portable::dynamic::clear_capture_writes;
        clear_capture_writes();

        let cb_id = register_dynamic_callback(Box::new(ChainingCallback));
        let mut g = Grammar::new();
        // Branch 1: dynamic (writes :cont, matches "A") then "!" (fails).
        let d1 = g.add_atom(Atom::Dynamic { callback_id: cb_id });
        let bang = g.add_atom(Atom::Str {
            pattern: "!".to_string(),
        });
        let b1 = g.add_atom(Atom::Sequence {
            atoms: vec![d1, bang],
        });
        // Branch 2: dynamic with a clean capture set: matches "A"
        // (cont empty), then "B".
        let d2 = g.add_atom(Atom::Dynamic { callback_id: cb_id });
        let bee = g.add_atom(Atom::Str {
            pattern: "B".to_string(),
        });
        let b2 = g.add_atom(Atom::Sequence {
            atoms: vec![d2, bee],
        });
        let root = g.add_atom(Atom::Alternative {
            atoms: vec![b1, b2],
        });
        g.root = root;

        // If the write leaked, d2 would dispatch to "B" and the parse
        // of "AB" would fail; with the rollback discipline it passes.
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&g, "AB", &mut arena);
        parser
            .parse()
            .unwrap_or_else(|e| panic!("failed-branch write leaked: {e:?}"));
    }

    /// The write channel converts to literal-text capture values that
    /// read back exactly, independent of the input.
    #[test]
    fn writes_read_back_as_text_values() {
        note_capture_writes(vec![("k".to_string(), "literal".to_string())]);
        let mut caps = CaptureState::new();
        drain_capture_writes_into(&mut caps);
        let input = "completely unrelated input";
        assert_eq!(
            caps.get("k").map(|v| v.get_text(input).into_owned()),
            Some("literal".to_string())
        );
    }
}

mod dispatch_cache {
    use crate::portable::arena::AstArena;
    use crate::portable::dynamic::{
        begin_parse, register_dynamic_callback, DynamicCallback, DynamicContext,
    };
    use crate::portable::grammar::{Atom, Grammar};
    use crate::portable::parser::PortableParser;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CALLS: AtomicUsize = AtomicUsize::new(0);

    struct CountingCallback;

    impl DynamicCallback for CountingCallback {
        fn resolve(&self, ctx: &DynamicContext) -> Option<Atom> {
            CALLS.fetch_add(1, Ordering::SeqCst);
            let pattern = if ctx.get_capture_text("m").is_some() {
                "B"
            } else {
                "A"
            };
            Some(Atom::Str {
                pattern: pattern.to_string(),
            })
        }
        fn description(&self) -> &str {
            "counting dispatcher"
        }
    }

    /// The same (callback, position, captures) must resolve ONCE per
    /// parse: backtracking re-invocations hit the dispatch cache
    /// (parsanol-ruby#80 item 2), and a capture-state change at the
    /// same position still re-resolves.
    #[test]
    fn identical_dispatches_resolve_once_per_parse() {
        let cb_id = register_dynamic_callback(Box::new(CountingCallback));

        // Grammar: seq(dynamic, dynamic) — same position? No: two
        // dynamic atoms at DIFFERENT positions. Use a repetition-free
        // shape: alt(seq(d, "!"), seq(d, "B")) — d at pos 0 twice.
        let mut g = Grammar::new();
        let d = g.add_atom(Atom::Dynamic { callback_id: cb_id });
        let bang = g.add_atom(Atom::Str {
            pattern: "!".to_string(),
        });
        let bee = g.add_atom(Atom::Str {
            pattern: "B".to_string(),
        });
        let b1 = g.add_atom(Atom::Sequence {
            atoms: vec![d, bang],
        });
        let b2 = g.add_atom(Atom::Sequence {
            atoms: vec![d, bee],
        });
        let root = g.add_atom(Atom::Alternative {
            atoms: vec![b1, b2],
        });
        g.root = root;

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&g, "AB", &mut arena);
        parser.parse().expect("branch 2 parses");

        // Without the cache the alternative's second branch would
        // re-invoke the callback for position 0; with it, one
        // resolution per parse — unless capture state differed (it
        // did not: both branches start at 0 with no captures).
        let calls = CALLS.load(Ordering::SeqCst);
        assert_eq!(
            calls, 1,
            "same (cb, pos, captures) must resolve once per parse"
        );

        // A new parse starts clean: the cache is per-parse.
        begin_parse("AB");
        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&g, "AB", &mut arena);
        parser.parse().expect("second parse");
        assert_eq!(CALLS.load(Ordering::SeqCst), calls + 1);
    }
}
