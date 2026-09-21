//! Incremental reparse latency benchmark (TODO.perf/8).
//!
//! Builds a ~600 KB line-based document, runs an initial full parse,
//! then simulates 100 single-line keystroke edits spread across the
//! document. Reports per-edit incremental latency (mean/p95/max), the
//! full-reparse baseline, and the speedup — and asserts per edit that
//! the incremental tree equals a full re-parse, so the benchmark
//! doubles as a correctness gate.
//!
//! Run quiet for absolute numbers:
//!
//!     cargo run --release -p parsanol --example incremental_bench

use parsanol::portable::arena::AstArena;
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

fn flatten(
    node: &parsanol::portable::ast::AstNode,
    arena: &AstArena,
    input: &str,
    out: &mut Vec<String>,
) {
    use parsanol::portable::ast::AstNode;
    match node {
        AstNode::InputRef { offset, length } => {
            out.push(input[*offset as usize..*offset as usize + *length as usize].to_string())
        }
        AstNode::Array { pool_index, length } => {
            for child in arena.get_array(*pool_index as usize, *length as usize) {
                flatten(&child, arena, input, out);
            }
        }
        AstNode::Hash { pool_index, length } => {
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

fn main() {
    let grammar = kv_grammar();
    let lines: Vec<String> = (0..60_000).map(|i| format!("key{i}={}\n", i * 7)).collect();
    let mut input = lines.concat();
    println!("document: {:.0} KiB", input.len() as f64 / 1024.0);

    let mut session = IncrementalParser::owned(grammar.clone());

    let t0 = Instant::now();
    let mut arena = AstArena::new();
    session
        .parse(&input, &mut arena)
        .unwrap_or_else(|e| panic!("initial parse: {e:?}"));
    let initial = t0.elapsed();
    println!("initial full parse: {:.2?}", initial);

    // 100 simulated keystrokes: rewrite one line's value, spread
    // uniformly across the document, shrinking and growing it by
    // turns so length changes are exercised.
    let line_count = 60_000usize;
    let mut latencies = Vec::with_capacity(100);
    let mut full_baseline = std::time::Duration::ZERO;

    for k in 0..100 {
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

        let t = Instant::now();
        let mut arena = AstArena::new();
        let result = session
            .parse_with_edit(
                &input,
                &mut arena,
                Edit::replace(offset, old_len, new_line.len()),
            )
            .expect("incremental parse");
        let dt = t.elapsed();
        latencies.push(dt);
        if k % 25 == 0 {
            let (reused, invalidated) = (
                result.reused_cache_entries,
                result.invalidated_cache_entries,
            );
            eprintln!("  edit {k}: retained={reused} dropped={invalidated}");
        }

        // Correctness gate: the incremental tree equals a full parse.
        let t = Instant::now();
        let reference = full_flatten(&grammar, &input);
        full_baseline += t.elapsed();
        let mut got = Vec::new();
        flatten(&result.ast, &arena, &input, &mut got);
        assert_eq!(
            got, reference,
            "tree mismatch at keystroke {k} (line {line})"
        );
    }

    latencies.sort();
    let mean = latencies.iter().sum::<std::time::Duration>() / latencies.len() as u32;
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let max = latencies[latencies.len() - 1];
    let full_mean = full_baseline / 100;
    println!(
        "keystroke re-parse: mean {:.2?}  p95 {:.2?}  max {:.2?}",
        mean, p95, max
    );
    println!(
        "full re-parse baseline: mean {:.2?}  → speedup {:.1}x",
        full_mean,
        full_mean.as_secs_f64() / mean.as_secs_f64()
    );
    let (hits, misses, rate) = session.cache_stats();
    println!("session cache: {hits} hits, {misses} misses ({rate:.0}% hit rate)");
}
