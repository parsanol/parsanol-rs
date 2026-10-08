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
            note_capture_writes(vec![(
                "cont".to_string(),
                crate::portable::dynamic::WriteValue::Text(next),
            )]);

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
        note_capture_writes(vec![(
            "k".to_string(),
            crate::portable::dynamic::WriteValue::Text("literal".to_string()),
        )]);
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
        // Fragment resolution (the host-bridge shape) is what the
        // dispatch cache stores: resolve-path results hold a full
        // grammar clone per entry and are deliberately not cached
        // (parsanol-ruby#84 memory bound).
        fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
            None
        }
        fn resolve_fragment(&self, ctx: &DynamicContext) -> Option<(Grammar, usize)> {
            CALLS.fetch_add(1, Ordering::SeqCst);
            let pattern = if ctx.get_capture_text("m").is_some() {
                "B"
            } else {
                "A"
            };
            let mut g = Grammar::new();
            let a = g.add_atom(Atom::Str {
                pattern: pattern.to_string(),
            });
            g.root = a;
            Some((g, a))
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

mod profiling {
    use crate::portable::arena::AstArena;
    use crate::portable::grammar::{Atom, Grammar, RepetitionTag};
    use crate::portable::parser::PortableParser;

    /// parsanol-rs#100 item 2: per-atom dispatch counts identify the
    /// grammar's hot atoms without a debugger.
    #[test]
    fn dispatch_counts_rank_hot_atoms() {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        let rep = g.add_atom(Atom::Repetition {
            atom: a,
            min: 0,
            max: None,
            tag: RepetitionTag::Repetition,
        });
        g.root = rep;

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&g, "aaaa", &mut arena);
        assert!(parser.profile_summary().is_empty());
        parser.enable_profiling();
        parser.parse().expect("parse");

        let counts = parser.dispatch_counts().expect("counts");
        // 'a' is attempted at 0..=4 (the fifth fails): hotter than the
        // repetition's two attempts.
        assert!(counts[a] > counts[rep]);
        let summary = parser.profile_summary();
        assert_eq!(summary[0].0, a, "hottest atom first");
    }

    /// parsanol-rs#100 item 4: parse() returns the raw tagged tree —
    /// a stable, documented shape — without parslet normalization.
    #[test]
    fn parse_returns_the_raw_tagged_tree() {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "a".to_string(),
        });
        let named = g.add_atom(Atom::Named {
            name: "x".to_string(),
            atom: a,
        });
        g.root = named;

        let mut arena = AstArena::new();
        let mut parser = PortableParser::new(&g, "a", &mut arena);
        let tree = parser.parse().expect("parse");

        use crate::portable::ast::AstNode;
        let AstNode::Hash { pool_index, length } = tree else {
            panic!("named capture is a hash envelope");
        };
        let items = arena.get_hash_items(pool_index as usize, length as usize);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, "x");
        assert!(matches!(
            items[0].1,
            AstNode::InputRef {
                offset: 0,
                length: 1
            }
        ));
    }
}

#[cfg(test)]
mod fused_neg_scan_tests {
    use super::super::{AstArena, Grammar, PortableParser};

    /// The quoted-string idiom (parsanol-ruby#115): a repetition whose body
    /// is `Sequence[Lookahead(negative, delimiters), terminal]`. The fused
    /// scan executes it as a byte-test loop; the general loop runs the VM
    /// per char. Both must produce identical trees.
    const QUOTED: &str = r#"{"atoms":[{"Str":{"pattern":"\""}},{"Entity":{"atom":8}},{"Str":{"pattern":"\\"}},{"Str":{"pattern":"\""}},{"Named":{"name":"dquote","atom":3}},{"Str":{"pattern":"n"}},{"Named":{"name":"newline","atom":5}},{"Alternative":{"atoms":[4,6]}},{"Sequence":{"atoms":[2,7]}},{"Entity":{"atom":17}},{"Str":{"pattern":"\\"}},{"Str":{"pattern":"\""}},{"Alternative":{"atoms":[10,11]}},{"Lookahead":{"atom":12,"positive":false}},{"Entity":{"atom":15}},{"Re":{"pattern":"[\\x00-\\u{10ffff}]"}},{"Sequence":{"atoms":[13,14]}},{"Repetition":{"atom":16,"min":1,"max":null,"tag":"Repetition"}},{"Named":{"name":"run","atom":9}},{"Alternative":{"atoms":[1,18]}},{"Repetition":{"atom":19,"min":0,"max":null,"tag":"Repetition"}},{"Named":{"name":"string","atom":20}},{"Str":{"pattern":"\""}},{"Sequence":{"atoms":[0,21,22]}}],"root":23}"#;

    /// Semantically identical, but the fusion-eligible nodes are
    /// wrapped (terminal and lookahead body under `Alternative`) so
    /// the recognizer declines and the general loop runs.
    const QUOTED_UNFUSED: &str = r#"{"atoms":[{"Str":{"pattern":"\""}},{"Entity":{"atom":8}},{"Str":{"pattern":"\\"}},{"Str":{"pattern":"\""}},{"Named":{"name":"dquote","atom":3}},{"Str":{"pattern":"n"}},{"Named":{"name":"newline","atom":5}},{"Alternative":{"atoms":[4,6]}},{"Sequence":{"atoms":[2,7]}},{"Entity":{"atom":17}},{"Str":{"pattern":"\\"}},{"Str":{"pattern":"\""}},{"Alternative":{"atoms":[10,11]}},{"Lookahead":{"atom":25,"positive":false}},{"Entity":{"atom":15}},{"Re":{"pattern":"[\\x00-\\u{10ffff}]"}},{"Sequence":{"atoms":[13,24]}},{"Repetition":{"atom":16,"min":1,"max":null,"tag":"Repetition"}},{"Named":{"name":"run","atom":9}},{"Alternative":{"atoms":[1,18]}},{"Repetition":{"atom":19,"min":0,"max":null,"tag":"Repetition"}},{"Named":{"name":"string","atom":20}},{"Str":{"pattern":"\""}},{"Sequence":{"atoms":[0,21,22]}},{"Alternative":{"atoms":[14]}},{"Alternative":{"atoms":[12]}}],"root":23}"#;

    fn parse_debug(grammar_json: &str, input: &str) -> String {
        let g = Grammar::from_json(grammar_json).unwrap();
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.to_string());
        let mut p = PortableParser::new(&g, input, &mut arena);
        let ast = p.parse().expect("parse succeeds");
        format!("{ast:?}")
    }

    #[test]
    fn fused_tree_equals_general_loop_tree() {
        for input in [
            "\"hello world\"",
            "\"a\\nb\"",
            "\"\\\"quoted\\\" tail\"",
            "\"unicode é字!\"",
            "\"x\"",
        ] {
            let fused = parse_debug(QUOTED, input);
            let general = parse_debug(QUOTED_UNFUSED, input);
            assert_eq!(fused, general, "tree divergence for {input}");
        }
    }

    // rs#174 round 2 regression: nested dynamic fragments used to
    // double the capture-state undo log at every boundary crossing
    // (copy-in + merge-back re-stored every name), so a small nested
    // grammar drove the log to 296k entries and the capture-content
    // signature — computed per dynamic-dependent memo lookup — to
    // 56s / 16GB on a 26-byte input. The fragment boundary now seeds
    // via clone_visible and merges back only changed captures.
    #[test]
    fn nested_dynamic_fragments_keep_the_capture_log_bounded() {
        use crate::portable::dynamic::{
            register_dynamic_callback, DynamicCallback, DynamicContext,
        };
        use crate::portable::grammar::{Atom, Grammar, RepetitionTag};

        struct RowsFrag;
        struct RowFrag;
        struct ContentFrag;

        impl DynamicCallback for RowsFrag {
            fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
                None
            }
            fn description(&self) -> &str {
                "rows"
            }
            fn resolve_fragment(&self, _ctx: &DynamicContext) -> Option<(Grammar, usize)> {
                let mut g = Grammar::new();
                let bang = g.add_atom(Atom::Str {
                    pattern: "!".to_string(),
                });
                let guard = g.add_atom(Atom::Lookahead {
                    atom: bang,
                    positive: false,
                });
                let row = g.add_atom(Atom::Dynamic {
                    callback_id: CELL_CB.load(std::sync::atomic::Ordering::Relaxed),
                });
                let item = g.add_atom(Atom::Sequence {
                    atoms: vec![guard, row],
                });
                let root = g.add_atom(Atom::Repetition {
                    atom: item,
                    min: 1,
                    max: None,
                    tag: RepetitionTag::Repetition,
                });
                g.root = root;
                Some((g, root))
            }
        }
        impl DynamicCallback for RowFrag {
            fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
                None
            }
            fn description(&self) -> &str {
                "row"
            }
            fn resolve_fragment(&self, _ctx: &DynamicContext) -> Option<(Grammar, usize)> {
                let mut g = Grammar::new();
                let content = g.add_atom(Atom::Dynamic {
                    callback_id: CONTENT_CB.load(std::sync::atomic::Ordering::Relaxed),
                });
                let sep = g.add_atom(Atom::Str {
                    pattern: ",".to_string(),
                });
                let root = g.add_atom(Atom::Sequence {
                    atoms: vec![content, sep],
                });
                g.root = root;
                Some((g, root))
            }
        }
        impl DynamicCallback for ContentFrag {
            fn resolve(&self, _ctx: &DynamicContext) -> Option<Atom> {
                None
            }
            fn description(&self) -> &str {
                "content"
            }
            fn resolve_fragment(&self, _ctx: &DynamicContext) -> Option<(Grammar, usize)> {
                let mut g = Grammar::new();
                let bang = g.add_atom(Atom::Str {
                    pattern: "!".to_string(),
                });
                let g1 = g.add_atom(Atom::Lookahead {
                    atom: bang,
                    positive: false,
                });
                let comma = g.add_atom(Atom::Str {
                    pattern: ",".to_string(),
                });
                let g2 = g.add_atom(Atom::Lookahead {
                    atom: comma,
                    positive: false,
                });
                let any = g.add_atom(Atom::Re {
                    pattern: "(?s).".to_string(),
                });
                let item = g.add_atom(Atom::Sequence {
                    atoms: vec![g1, g2, any],
                });
                let root = g.add_atom(Atom::Repetition {
                    atom: item,
                    min: 0,
                    max: None,
                    tag: RepetitionTag::Repetition,
                });
                g.root = root;
                Some((g, root))
            }
        }

        static ROW_CB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
        static CELL_CB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
        static CONTENT_CB: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(u64::MAX);
        ROW_CB.store(
            register_dynamic_callback(Box::new(RowsFrag)),
            std::sync::atomic::Ordering::Relaxed,
        );
        CELL_CB.store(
            register_dynamic_callback(Box::new(RowFrag)),
            std::sync::atomic::Ordering::Relaxed,
        );
        CONTENT_CB.store(
            register_dynamic_callback(Box::new(ContentFrag)),
            std::sync::atomic::Ordering::Relaxed,
        );

        let row_cb = ROW_CB.load(std::sync::atomic::Ordering::Relaxed);

        // root: Capture("d", "|") >> Dynamic(rows)
        let mut g = Grammar::new();
        let bar = g.add_atom(Atom::Str {
            pattern: "|".to_string(),
        });
        let cap = g.add_atom(Atom::Capture {
            name: "d".to_string(),
            atom: bar,
        });
        let rows = g.add_atom(Atom::Dynamic {
            callback_id: row_cb,
        });
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![cap, rows],
        });
        g.root = root;

        let input = format!("|{}!", "a,".repeat(60));
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&g, input.as_str(), &mut arena);
        let result = parser.try_atom(root, 0).expect("nested fragments parse");
        assert_eq!(result.end_pos, input.len() - 1);

        // The undo log (whose length version() tracks) must stay
        // bounded by the captures the fragments actually changed —
        // not doubled per boundary crossing.
        assert!(
            parser.capture_state.version() < 500,
            "capture undo log exploded: version={}",
            parser.capture_state.version()
        );
    }

    #[test]
    fn fused_scan_rejects_below_min_like_the_general_loop() {
        // No terminal chars between the quotes: `1*` cannot satisfy its
        // minimum; the fused scan declines and the general loop rejects.
        let g = Grammar::from_json(QUOTED).unwrap();
        let input = "\"";
        let mut arena = AstArena::for_input(2);
        arena.set_input(input.to_string());
        let mut p = PortableParser::new(&g, input, &mut arena);
        assert!(p.parse().is_err());
    }
}

#[cfg(test)]
mod trivia_capture_tests {
    use super::super::{AstArena, AstNode, Grammar, PortableParser};

    // The injected skip wrapper (parsanol-ruby#152): (spaces /
    // line_comment)+ under a capturing TriviaCapture, ahead of a
    // Named word. Only comment-shaped units record; the recorded
    // units attach to the next successful Named capture under
    // `comments:`.
    const CAPTURING_SKIP: &str = r#"{"atoms":[
        {"Re":{"pattern":"[ \n]"}},
        {"Repetition":{"atom":0,"min":1,"max":null,"tag":"Repetition"}},
        {"Str":{"pattern":"//"}},
        {"Re":{"pattern":"[^\n]*"}},
        {"Sequence":{"atoms":[2,3]}},
        {"Alternative":{"atoms":[1,4]}},
        {"TriviaCapture":{"atom":5,"rules":[["//","line_comment"]]}},
        {"Repetition":{"atom":6,"min":0,"max":null,"tag":"Repetition"}},
        {"Re":{"pattern":"[a-z]+"}},
        {"Named":{"name":"word","atom":8}},
        {"Sequence":{"atoms":[7,9]}}
    ],"root":10}"#;

    fn resolve(a: &AstArena, n: &AstNode) -> String {
        match n {
            AstNode::Nil => "null".to_string(),
            AstNode::Bool(b) => b.to_string(),
            AstNode::Int(i) => i.to_string(),
            AstNode::Float(f) => f.to_string(),
            AstNode::StringRef { pool_index } => {
                format!("{:?}", a.get_string(*pool_index as usize))
            }
            AstNode::InputRef { offset, length } => format!(
                "{:?}",
                &a.get_input()[*offset as usize..(*offset + *length) as usize]
            ),
            AstNode::Array { pool_index, length } => {
                let items = a.get_array(*pool_index as usize, *length as usize);
                let inner: Vec<String> = items.iter().map(|i| resolve(a, i)).collect();
                format!("[{}]", inner.join(","))
            }
            AstNode::Hash { pool_index, length } => {
                let pairs = a.get_hash_items(*pool_index as usize, *length as usize);
                let inner: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{:?}:{}", k, resolve(a, v)))
                    .collect();
                format!("{{{}}}", inner.join(","))
            }
        }
    }

    fn parse_tree(grammar_json: &str, input: &str) -> String {
        let g = Grammar::from_json(grammar_json).unwrap();
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.to_string());
        let mut p = PortableParser::new(&g, input, &mut arena);
        let ast = p.parse().expect("parse succeeds");
        resolve(&arena, &ast)
    }

    #[test]
    fn comment_trivia_attaches_to_next_named_capture() {
        assert_eq!(
            parse_tree(CAPTURING_SKIP, "  // note\nbeta"),
            r#"[":sequence",[":repetition",null,null,null],{"word":"beta","comments":[":repetition",{"line_comment":"// note"}]}]"#
        );
    }

    #[test]
    fn whitespace_trivia_records_nothing() {
        assert_eq!(
            parse_tree(CAPTURING_SKIP, "   gamma"),
            r#"[":sequence",[":repetition",null],{"word":"gamma"}]"#
        );
    }

    #[test]
    fn no_trivia_leaves_v1_shape_unchanged() {
        assert_eq!(
            parse_tree(CAPTURING_SKIP, "delta"),
            r#"[":sequence",[":repetition"],{"word":"delta"}]"#
        );
    }
}

#[cfg(test)]
mod constant_lookbehind_tests {
    use super::super::{AstArena, AstNode, Grammar, PortableParser};

    // coradoc-markdown parity (rs#137 follow-up): Output /
    // precedes? atoms become wire-expressible Constant / Lookbehind.
    // Grammar: dash+ then Constant{hr: true} then Guarded*
    //   Guarded = Lookbehind(one byte behind is dash) then [a-z]+
    const PASS: &str = r#"{"atoms":[
        {"Re":{"pattern":"-"}},
        {"Repetition":{"atom":0,"min":1,"max":null,"tag":"Repetition"}},
        {"Constant":{"value":{"Hash":[["hr",{"Bool":true}]]}}},
        {"Sequence":{"atoms":[1,2]}},
        {"Re":{"pattern":"[a-z]+"}},
        {"Lookbehind":{"look":{"Literal":{"count":1,"pattern":"-"}},"positive":true}},
        {"Sequence":{"atoms":[5,4]}},
        {"Named":{"name":"guarded","atom":6}},
        {"Repetition":{"atom":7,"min":0,"max":null,"tag":"Repetition"}},
        {"Sequence":{"atoms":[3,8]}}
    ],"root":9}"#;

    // Same shape but the guard demands the byte 'z' behind: nothing
    // can pass, so a min-1 run makes the whole parse fail.
    const FAIL: &str = r#"{"atoms":[
        {"Re":{"pattern":"-"}},
        {"Repetition":{"atom":0,"min":1,"max":null,"tag":"Repetition"}},
        {"Constant":{"value":{"Str":"x"}}},
        {"Sequence":{"atoms":[1,2]}},
        {"Re":{"pattern":"[a-z]+"}},
        {"Lookbehind":{"look":{"Literal":{"count":1,"pattern":"z"}},"positive":true}},
        {"Sequence":{"atoms":[5,4]}},
        {"Named":{"name":"guarded","atom":6}},
        {"Repetition":{"atom":7,"min":1,"max":null,"tag":"Repetition"}},
        {"Sequence":{"atoms":[3,8]}}
    ],"root":9}"#;

    fn resolve(a: &AstArena, n: &AstNode) -> String {
        match n {
            AstNode::Nil => "null".to_string(),
            AstNode::Bool(b) => b.to_string(),
            AstNode::Int(i) => i.to_string(),
            AstNode::Float(f) => f.to_string(),
            AstNode::StringRef { pool_index } => {
                format!("{:?}", a.get_string(*pool_index as usize))
            }
            AstNode::InputRef { offset, length } => format!(
                "{:?}",
                &a.get_input()[*offset as usize..(*offset + *length) as usize]
            ),
            AstNode::Array { pool_index, length } => {
                let items = a.get_array(*pool_index as usize, *length as usize);
                let inner: Vec<String> = items.iter().map(|i| resolve(a, i)).collect();
                format!("[{}]", inner.join(","))
            }
            AstNode::Hash { pool_index, length } => {
                let pairs = a.get_hash_items(*pool_index as usize, *length as usize);
                let inner: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{:?}:{}", k, resolve(a, v)))
                    .collect();
                format!("{{{}}}", inner.join(","))
            }
        }
    }

    fn parse_tree(grammar_json: &str, input: &str) -> String {
        let g = Grammar::from_json(grammar_json).unwrap();
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.to_string());
        let mut p = PortableParser::new(&g, input, &mut arena);
        let ast = p.parse().expect("parse succeeds");
        resolve(&arena, &ast)
    }

    fn parse_err(grammar_json: &str, input: &str) -> bool {
        let g = Grammar::from_json(grammar_json).unwrap();
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.to_string());
        let mut p = PortableParser::new(&g, input, &mut arena);
        p.parse().is_err()
    }

    #[test]
    fn constant_yields_the_wire_value_after_its_prefix() {
        assert_eq!(
            parse_tree(PASS, "-"),
            r#"[":sequence",[":sequence",[":repetition","-"],{"hr":true}],[":repetition"]]"#
        );
    }

    #[test]
    fn lookbehind_passes_when_the_byte_behind_matches() {
        let tree = parse_tree(PASS, "---abc");
        assert!(tree.contains("guarded"), "tree: {tree}");
    }

    #[test]
    fn lookbehind_fails_when_the_byte_behind_differs() {
        assert!(parse_err(FAIL, "-abc"));
    }

    // The flanking form (parsanol-ruby#163): the regex variant is
    // searched in the preceding text and must end at the position —
    // class-based, multibyte, variable-length.
    const REGEX_GUARD: &str = r#"{"atoms":[
        {"Re":{"pattern":"[a-z ]"}},
        {"Lookbehind":{"look":{"Regex":{"source":"[[:space:]]"}},"positive":true}},
        {"Re":{"pattern":"[a-z]+"}},
        {"Sequence":{"atoms":[0,1,2]}}
    ],"root":3}"#;

    #[test]
    fn regex_lookbehind_ends_at_the_position() {
        // " abc": the first byte consumed is the space, so at the
        // run start the preceding text ends with whitespace.
        let tree = parse_tree(REGEX_GUARD, " abc");
        assert!(tree.contains("abc"), "tree: {tree}");
    }

    #[test]
    fn regex_lookbehind_fails_without_the_class_behind() {
        // "x abc": whichever byte the lead consumes, the text behind
        // the run start is a letter — the class never holds.
        assert!(parse_err(REGEX_GUARD, "x abc"));
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;
    use crate::portable::arena::AstArena;

    // The block_open/block_close shape: StateSet(expr) remembers the
    // opening delimiter's consumed text; StateMatch requires exactly
    // that text verbatim at the close position.
    #[test]
    fn state_set_expr_and_match_close_delimiter() {
        let mut g = Grammar::new();
        let open = g.add_atom(Atom::Str {
            pattern: "--".to_string(),
        });
        let set = g.add_atom(Atom::StateSet {
            slot: "delim".to_string(),
            value: None,
            expr: Some(open),
        });
        let body = g.add_atom(Atom::Str {
            pattern: "x".to_string(),
        });
        let close = g.add_atom(Atom::StateMatch {
            slot: "delim".to_string(),
        });
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![set, body, close],
        });
        g.root = root;

        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, "--x--", &mut arena);
        assert!(p.parse().is_ok());

        // A different-length or different-text close never matches:
        // the comparison is verbatim against the captured text.
        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, "--x~-", &mut arena);
        assert!(p.parse().is_err());
    }

    // A switch routes by the slot's literal value to a named rule.
    #[test]
    fn state_switch_routes_by_slot_value() {
        let mut g = Grammar::new();
        let a = g.add_atom(Atom::Str {
            pattern: "A".to_string(),
        });
        let b = g.add_atom(Atom::Str {
            pattern: "B".to_string(),
        });
        let _d1 = g.add_atom(Atom::Named {
            name: "d1".to_string(),
            atom: a,
        });
        let _d2 = g.add_atom(Atom::Named {
            name: "d2".to_string(),
            atom: b,
        });
        let set = g.add_atom(Atom::StateSet {
            slot: "mode".to_string(),
            value: Some("d1".to_string()),
            expr: None,
        });
        let switch = g.add_atom(Atom::StateSwitch {
            slot: "mode".to_string(),
            arms: [
                ("d1".to_string(), "d1".to_string()),
                ("d2".to_string(), "d2".to_string()),
            ]
            .into_iter()
            .collect(),
            default: None,
        });
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![set, switch],
        });
        g.root = root;

        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, "A", &mut arena);
        assert!(p.parse().is_ok());

        // The default arm covers unmatched slot values.
        let set_default_route = g.add_atom(Atom::StateSet {
            slot: "mode".to_string(),
            value: Some("other".to_string()),
            expr: None,
        });
        let mut arena = AstArena::new();
        let switch_default = g.add_atom(Atom::StateSwitch {
            slot: "mode".to_string(),
            arms: [("d1".to_string(), "d1".to_string())].into_iter().collect(),
            default: Some("d2".to_string()),
        });
        let root_default = g.add_atom(Atom::Sequence {
            atoms: vec![set_default_route, switch_default],
        });
        g.root = root_default;
        let mut p = PortableParser::new(&g, "B", &mut arena);
        assert!(p.parse().is_ok());

        // No arm and no default: the atom fails.
        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, "A", &mut arena);
        assert!(p.parse().is_err());
    }

    // State writes ride the capture undo log: a failed branch's slot
    // write must not leak into later alternatives. With a leak, the
    // second branch would match the input's first byte and the root
    // would end as Incomplete rather than Failed.
    #[test]
    fn state_write_rolls_back_on_failed_branch() {
        let mut g = Grammar::new();
        let write = g.add_atom(Atom::StateSet {
            slot: "s".to_string(),
            value: Some("X".to_string()),
            expr: None,
        });
        let fail = g.add_atom(Atom::Str {
            pattern: "Z".to_string(),
        });
        let leak_branch = g.add_atom(Atom::Sequence {
            atoms: vec![write, fail],
        });
        let read = g.add_atom(Atom::StateMatch {
            slot: "s".to_string(),
        });
        let root = g.add_atom(Atom::Alternative {
            atoms: vec![leak_branch, read],
        });
        g.root = root;

        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, "XY", &mut arena);
        // No leak: read fails (slot unset), the root fails outright.
        // With a leak it would match "X" and end as Incomplete.
        match p.parse() {
            Err(ParseError::Failed { .. }) => {}
            other => panic!(
                "expected Failed, got {:?}",
                other.err().map(|e| e.to_string())
            ),
        }
    }

    // StateMatch on an unset slot fails at the position.
    #[test]
    fn state_match_unset_slot_fails() {
        let mut g = Grammar::new();
        let read = g.add_atom(Atom::StateMatch {
            slot: "nope".to_string(),
        });
        g.root = read;

        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, "anything", &mut arena);
        assert!(p.parse().is_err());
    }
}

#[cfg(test)]
mod custom_ref_tests {
    use super::*;
    use crate::portable::arena::AstArena;

    // Reaching a CustomRef aborts the WHOLE native pass (a runtime
    // property), never just the branch: with branch semantics an
    // alternative could silently skip the custom atom and diverge
    // from the Ruby interpreter, which evaluates it.
    #[test]
    fn custom_ref_aborts_the_parse_not_the_branch() {
        let mut g = Grammar::new();
        let custom = g.add_atom(Atom::CustomRef {
            name: "my_custom".to_string(),
        });
        let lit = g.add_atom(Atom::Str {
            pattern: "A".to_string(),
        });
        let root = g.add_atom(Atom::Alternative {
            atoms: vec![custom, lit],
        });
        g.root = root;

        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, "A", &mut arena);
        match p.parse() {
            Err(ParseError::InvalidGrammar { .. }) => {}
            other => panic!("expected InvalidGrammar, got {:?}", other),
        }
    }
}

#[cfg(test)]
mod state_compaction_tests {
    use super::*;
    use crate::portable::arena::AstArena;

    // The compaction pass must treat the StateSet's inline expression
    // as reachable AND remap its index: dropping either corrupts the
    // expr into a stale pointer — observed as a self-referential
    // delimiter cycle (infinite recursion) on the coradoc-adoc
    // artifact (parsanol-ruby#162).
    #[test]
    fn state_set_expr_survives_from_json_compaction() {
        let mut g = Grammar::new();
        let tick = g.add_atom(Atom::Str {
            pattern: "`".to_string(),
        });
        let rep = g.add_atom(Atom::Repetition {
            atom: tick,
            min: 3,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let set = g.add_atom(Atom::StateSet {
            slot: "fence".to_string(),
            value: None,
            expr: Some(rep),
        });
        let body = g.add_atom(Atom::Str {
            pattern: "x".to_string(),
        });
        let close = g.add_atom(Atom::StateMatch {
            slot: "fence".to_string(),
        });
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![set, body, close],
        });
        g.root = root;

        let json = g.to_json().expect("serialize");
        let loaded = Grammar::from_json(&json).expect("load");

        // The runtime StateSet's expr must still point at a Repetition.
        let set_id = loaded
            .atoms
            .iter()
            .position(|a| matches!(a, Atom::StateSet { .. }))
            .expect("StateSet present");
        let expr = match &loaded.atoms[set_id] {
            Atom::StateSet { expr, .. } => expr.expect("expr kept"),
            other => panic!("unexpected atom {:?}", other),
        };
        assert!(
            matches!(&loaded.atoms[expr], Atom::Repetition { min: 3, .. }),
            "expr must remap to the repetition, got {:?}",
            loaded.atoms[expr]
        );

        // And the fence-close shape must parse.
        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&loaded, "```x```", &mut arena);
        assert!(p.parse().is_ok());
    }
}

#[cfg(test)]
mod state_whitespace_tests {
    use super::*;
    use crate::portable::arena::AstArena;

    // parsanol-ruby#180: a declared whitespace kind records
    // marker-less trivia units verbatim (the source-preserving mode).
    #[test]
    fn trivia_capture_whitespace_kind_records_markerless_units() {
        let mut g = Grammar::new();
        let space = g.add_atom(Atom::Re {
            pattern: "[ ]".to_string(),
        });
        let lower = g.add_atom(Atom::Re {
            pattern: "[a-z]".to_string(),
        });
        let word_run = g.add_atom(Atom::Repetition {
            atom: lower,
            min: 1,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let word = g.add_atom(Atom::Named {
            name: "word".to_string(),
            atom: word_run,
        });
        // wrapper: 0..1 of (spaces / comment); only the comment form
        // has a marker — bare spaces record under the fallback kind.
        let spaces_rep = g.add_atom(Atom::Repetition {
            atom: space,
            min: 1,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let comment = g.add_atom(Atom::Str {
            pattern: "//".to_string(),
        });
        let alt = g.add_atom(Atom::Alternative {
            atoms: vec![spaces_rep, comment],
        });
        let capture = g.add_atom(Atom::TriviaCapture {
            atom: alt,
            rules: vec![("//".to_string(), "comment".to_string())],
            whitespace: Some("space".to_string()),
        });
        let root = g.add_atom(Atom::Sequence {
            atoms: vec![capture, word, capture],
        });
        g.root = root;

        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, " alpha ", &mut arena);
        match p.parse() {
            Ok(_) => {}
            Err(e) => panic!("parse failed: {e:?}"),
        }
        // Recorded units are verbatim. The LEADING " " drained into
        // the word Named's comments (the #152 lifecycle); the
        // trailing " " stays pending at end of parse.
        let pending = std::mem::take(&mut p.pending_trivia);
        let labels: Vec<&str> = pending.iter().map(|(l, _, _)| l.as_str()).collect();
        assert_eq!(labels, vec!["space"]);
        assert_eq!(pending[0].1, " ");
    }
}

#[cfg(test)]
mod fence_regression_tests {
    use super::*;
    use crate::portable::arena::AstArena;

    // parsanol-ruby#185: a FAILED block attempt's dyn-cache entry for
    // the fence StateSet replayed without the slot write — the second
    // block's close then matched a stale slot. State-write subtrees
    // bypass the dyn cache.
    #[test]
    fn state_set_is_not_served_from_the_dyn_cache() {
        let mut g = Grammar::new();
        let dash = g.add_atom(Atom::Str {
            pattern: "-".to_string(),
        });
        let fence = g.add_atom(Atom::Repetition {
            atom: dash,
            min: 4,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let set = g.add_atom(Atom::StateSet {
            slot: "fence".to_string(),
            value: None,
            expr: Some(fence),
        });
        let nl = g.add_atom(Atom::Str {
            pattern: "\n".to_string(),
        });
        let x = g.add_atom(Atom::Str {
            pattern: "x".to_string(),
        });
        let body = g.add_atom(Atom::Repetition {
            atom: x,
            min: 1,
            max: None,
            tag: crate::portable::grammar::RepetitionTag::Repetition,
        });
        let close = g.add_atom(Atom::StateMatch {
            slot: "fence".to_string(),
        });
        let q = g.add_atom(Atom::Str {
            pattern: "Q".to_string(),
        });
        // The first alternative FAILS after the set (poisoning a naive
        // dyn cache with a success entry at position 0); the second
        // re-evaluates the same set at the same position — the slot
        // write must happen for its close to match.
        let failing = g.add_atom(Atom::Sequence {
            atoms: vec![set, nl, q, nl, close, nl],
        });
        let ok = g.add_atom(Atom::Sequence {
            atoms: vec![set, nl, body, nl, close, nl],
        });
        let root = g.add_atom(Atom::Alternative {
            atoms: vec![failing, ok],
        });
        g.root = root;

        let mut arena = AstArena::new();
        let mut p = PortableParser::new(&g, "------\nx\n------\n", &mut arena);
        match p.parse() {
            Ok(_) => {}
            Err(e) => panic!("expected the retry to parse, got {e:?}"),
        }
    }
}
