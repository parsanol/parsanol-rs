//! TODO.perf/3 revisit check (2026-09-24): does opcode/atom dispatch
//! dominate on a real-shaped corpus? Profiles both engines with the
//! rs#100 counters. Run:
//!   cargo run --release -p parsanol --example prof_dispatch

use parsanol::portable::arena::AstArena;
use parsanol::portable::bytecode::compiler::compile;
use parsanol::portable::bytecode::vm::{BytecodeVM, VMConfig};
use parsanol::portable::grammar::{Atom, Grammar, RepetitionTag};
use parsanol::portable::parser::PortableParser;
use std::time::Instant;

fn kv_grammar() -> Grammar {
    let mut g = Grammar::new();
    let key = g.add_atom(Atom::Re { pattern: "[a-z][a-z0-9]*".to_string() });
    let val = g.add_atom(Atom::Re { pattern: "[0-9]+".to_string() });
    let eq = g.add_atom(Atom::Str { pattern: "=".to_string() });
    let nl = g.add_atom(Atom::Str { pattern: "\n".to_string() });
    let body = g.add_atom(Atom::Sequence { atoms: vec![key, eq, val, nl] });
    let named = g.add_atom(Atom::Named { name: "pair".to_string(), atom: body });
    let root = g.add_atom(Atom::Repetition { atom: named, min: 0, max: None, tag: RepetitionTag::Repetition });
    g.root = root;
    g
}

fn main() {
    let grammar = kv_grammar();
    let input: String = (0..60_000).map(|i| format!("key{i}={}\n", i * 7)).collect();
    println!("document: {:.0} KiB", input.len() as f64 / 1024.0);

    // Walker: per-atom dispatch counters.
    let mut arena = AstArena::new();
    {
        let mut parser = PortableParser::new(&grammar, &input, &mut arena);
        parser.enable_profiling();
        let t = Instant::now();
        parser.parse().expect("walker parse");
        let dt = t.elapsed();
        let counts = parser.profile_summary();
        let total: u64 = counts.iter().map(|(_, c)| c).sum();
        println!(
            "walker: {dt:.2?}  dispatches={total}  ({:.1} ns/dispatch)",
            dt.as_nanos() as f64 / total.max(1) as f64
        );
        for (atom, c) in counts.iter().rev().take(6) {
            println!("  atom {atom}: {c} ({:.0}%)", 100.0 * *c as f64 / total as f64);
        }
    }

    // Bytecode VM: per-opcode counters + backtrack count.
    let program = compile(grammar.clone()).expect("compile");
    let mut arena2 = AstArena::new();
    let mut vm = BytecodeVM::new(&program, &input, &mut arena2, VMConfig::default());
    vm.enable_profiling();
    let t = Instant::now();
    vm.run().expect("vm parse");
    let dt = t.elapsed();
    if let Some(counts) = vm.opcode_counts() {
        let total: u64 = counts.iter().sum();
        println!(
            "vm: {dt:.2?}  instructions={total}  ({:.1} ns/instr)  backtracks={}",
            dt.as_nanos() as f64 / total.max(1) as f64,
            vm.backtrack_count()
        );
        let mut ranked: Vec<(usize, u64)> =
            counts.iter().enumerate().map(|(i, c)| (i, *c)).filter(|(_, c)| *c > 0).collect();
        ranked.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        for (op, c) in ranked.iter().take(6) {
            println!("  opcode {op}: {c} ({:.0}%)", 100.0 * *c as f64 / total as f64);
        }
    }
}
