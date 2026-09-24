//! Retained-tier vs snapshot-tier keystroke latency (TODO.perf/9).
//!
//! Runs the identical document + 100-keystroke edit stream through
//! both incremental tiers:
//!
//! * snapshot tier (`parse_with_edit`): retention keeps only
//!   arena-free terminal entries; pool-backed nodes would need a
//!   cross-arena adoption copy (TODO.perf/4).
//! * retained tier (`parse_with_edit_retained`): the session owns its
//!   output arena, so FULL container entries survive with no copy and
//!   the previous parse tree persists alongside the memo window.
//!
//! Both tiers are gated per keystroke against a full re-parse.
//!
//!     cargo run --release -p parsanol --example incremental_retained_bench

use parsanol::portable::arena::AstArena;
use parsanol::portable::ast::AstNode;
use parsanol::portable::grammar::{Atom, Grammar, RepetitionTag};
use parsanol::portable::incremental::{Edit, IncrementalParser};
use parsanol::portable::parser::PortableParser;
use std::time::Instant;

fn kv_grammar() -> Grammar {
    let mut g = Grammar::new();
    let key = g.add_atom(Atom::Re {
        pattern: "[a-z][a-z0-9]*".to_string(),
    });
    let val = g.add_atom(Atom::Re {
        pattern: "[0-9]+".to_string(),
    });
    let eq = g.add_atom(Atom::Str {
        pattern: "=".to_string(),
    });
    let nl = g.add_atom(Atom::Str {
        pattern: "\n".to_string(),
    });
    let pair_body = g.add_atom(Atom::Sequence {
        atoms: vec![key, eq, val, nl],
    });
    let pair_rule = g.add_atom(Atom::Named {
        name: "pair".to_string(),
        atom: pair_body,
    });
    let root = g.add_atom(Atom::Repetition {
        atom: pair_rule,
        min: 0,
        max: None,
        tag: RepetitionTag::Repetition,
    });
    g.root = root;
    g
}

fn flatten(node: &AstNode, arena: &AstArena, input: &str, out: &mut Vec<String>) {
    match node {
        AstNode::InputRef { offset, length } => {
            out.push(input[*offset as usize..*offset as usize + *length as usize].to_string())
        }
        AstNode::Array {
            pool_index,
            length,
        } => {
            for child in arena.get_array(*pool_index as usize, *length as usize) {
                flatten(&child, arena, input, out);
            }
        }
        AstNode::Hash {
            pool_index,
            length,
        } => {
            for (_, v) in arena.get_hash_items(*pool_index as usize, *length as usize) {
                flatten(&v, arena, input, out);
            }
        }
        AstNode::StringRef { pool_index } => {
            out.push(arena.get_string(*pool_index as usize).to_string())
        }
        _ => out.push(format!("{node:?}")),
    }
}

fn full_flatten(grammar: &Grammar, input: &str) -> Vec<String> {
    let mut arena = AstArena::new();
    let mut parser = PortableParser::new(grammar, input, &mut arena);
    let tree = parser.parse().expect("full parse");
    let mut out = Vec::new();
    flatten(&tree, &arena, input, &mut out);
    out
}

/// One simulated edit: returns (input_after, edit).
fn keystroke(input: &mut String, k: usize, line_count: usize) -> (usize, usize, usize) {
    let line = (k * line_count / 100).min(line_count - 1);
    let offset: usize = input.lines().take(line).map(|l| l.len() + 1).sum();
    let rest = &input[offset..];
    let old_len = rest.find('\n').expect("line") + 1;
    let new_line = if k % 2 == 0 {
        format!("edited{k}={}\n", k)
    } else {
        format!("grown{k}={:0width$}\n", k, width = 12)
    };
    input.replace_range(offset..offset + old_len, &new_line);
    (offset, old_len, new_line.len())
}

fn run_tier(retained: bool, document: &str, grammar: &Grammar) -> Vec<std::time::Duration> {
    let mut input = document.to_owned();
    let line_count = input.lines().count();
    let mut session = IncrementalParser::owned(grammar.clone());
    if retained {
        session.parse_retained(&input).expect("initial parse");
    } else {
        let mut arena = AstArena::new();
        session.parse(&input, &mut arena).expect("initial parse");
    }

    let mut latencies = Vec::with_capacity(100);
    for k in 0..100 {
        let (offset, old_len, new_len) = keystroke(&mut input, k, line_count);
        let edit = Edit::replace(offset, old_len, new_len);

        let t = Instant::now();
        if retained {
            let result = session
                .parse_with_edit_retained(&input, edit)
                .expect("reparse");
            let dt = t.elapsed();
            let reference = full_flatten(grammar, &input);
            let mut got = Vec::new();
            flatten(&result.ast, session.retained_arena(), &input, &mut got);
            assert_eq!(got, reference, "retained tier tree mismatch at keystroke {k}");
            latencies.push(dt);
        } else {
            let mut arena = AstArena::new();
            let result = session
                .parse_with_edit(&input, &mut arena, edit)
                .expect("reparse");
            let dt = t.elapsed();
            let reference = full_flatten(grammar, &input);
            let mut got = Vec::new();
            flatten(&result.ast, &arena, &input, &mut got);
            assert_eq!(got, reference, "snapshot tier tree mismatch at keystroke {k}");
            latencies.push(dt);
        }
    }
    latencies
}

fn stats(latencies: &[std::time::Duration]) -> (std::time::Duration, std::time::Duration, std::time::Duration) {
    let mut sorted = latencies.to_vec();
    sorted.sort();
    let mean = sorted.iter().sum::<std::time::Duration>() / sorted.len() as u32;
    let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];
    let max = sorted[sorted.len() - 1];
    (mean, p95, max)
}

fn main() {
    let grammar = kv_grammar();
    let document: String = (0..60_000).map(|i| format!("key{i}={}\n", i * 7)).collect();
    println!("document: {:.0} KiB, 100 keystrokes", document.len() as f64 / 1024.0);

    // Warm both paths, then measure twice; report the second run to
    // reduce first-touch noise.
    for pass in 0..2 {
        let snapshot = run_tier(false, &document, &grammar);
        let retained = run_tier(true, &document, &grammar);
        if pass == 0 {
            continue;
        }
        let (sm, sp95, smax) = stats(&snapshot);
        let (rm, rp95, rmax) = stats(&retained);
        println!("snapshot tier: mean {sm:.2?}  p95 {sp95:.2?}  max {smax:.2?}");
        println!("retained tier: mean {rm:.2?}  p95 {rp95:.2?}  max {rmax:.2?}");
        println!(
            "retained speedup: {:.2}x mean, {:.2}x p95",
            sm.as_secs_f64() / rm.as_secs_f64(),
            sp95.as_secs_f64() / rp95.as_secs_f64()
        );
    }
}
