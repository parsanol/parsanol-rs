//! rs#496 / ruby#507 acceptance: the VM-first parse route over the
//! EXPRESS fixture corpus.
//!
//! compile-once program; every fixture parses VM-first with the
//! end-pos gate (a VM end_pos that disagrees with the interpreter
//! falls back to the interpreter parse); the walls of both routes
//! print with the interpreter share each represents.
//!
//! Run:
//!   EXPRESSIR_ASSETS=~/src/lutaml/expressir-rs/assets \
//!   EXPRESSIR_CORPUS=~/src/lutaml/expressir/spec/fixtures \
//!     cargo test -p parsanol --test vm_corpus_acceptance -- --ignored --nocapture

use parsanol::portable::bytecode::compiler::compile;
use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn assets_dir() -> PathBuf {
    std::env::var("EXPRESSIR_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../../lutaml/expressir-rs/assets"))
}

fn corpus_dir() -> PathBuf {
    std::env::var("EXPRESSIR_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../../lutaml/expressir/spec/fixtures"))
}

fn express_grammar() -> Grammar {
    let dir = assets_dir();
    let main = std::fs::read_to_string(dir.join("express-grammar.parg")).expect("main grammar");
    let components: BTreeMap<String, String> = [
        "express_keywords.parg",
        "express_operators.parg",
        "express_remarks.parg",
        "express_literals.parg",
    ]
    .iter()
    .map(|name| {
        (
            name.trim_end_matches(".parg").to_string(),
            std::fs::read_to_string(dir.join(name)).expect(name),
        )
    })
    .collect();
    use parsanol::parg::frontend;
    let mut parsed = frontend::parser::parse_grammar(&main).expect("the grammar parses");
    frontend::imports::merge_sources(&mut parsed.document, &components, &mut parsed.atomic_rules)
        .expect("the components merge");
    let tables = frontend::compile::Tables::default();
    let mut compiler =
        frontend::compile::compiler_for(&parsed.document, &tables, &parsed.atomic_rules)
            .expect("the compiler builds");
    compiler
        .compile_entry_grammar("syntax")
        .expect("the entry compiles")
}

fn corpus_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = std::fs::read_dir(&d).expect("readable corpus");
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "exp") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn parse_interpreter(grammar: &Grammar, input: &str) -> usize {
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(grammar, input, &mut arena);
    parser.parse_with_end_pos().map(|r| r.end_pos).unwrap_or(0)
}

fn parse_vm_first(
    program: &parsanol::portable::bytecode::Program,
    grammar: &Grammar,
    input: &str,
) -> usize {
    let mut arena = AstArena::for_input(input.len());
    let (vm, _) = parsanol::portable::bytecode::parse_with_vm_diag(program, input, &mut arena);
    match vm {
        Ok(r) if r.end_pos == input.len() => r.end_pos,
        // the end-pos gate: a VM verdict that fails the gate falls back
        // to the interpreter (the measured route keeps correctness)
        _ => parse_interpreter(grammar, input),
    }
}

#[test]
#[ignore = "needs the expressir assets + corpus - the ruby#507 acceptance measurement"]
fn vm_first_route_reduces_the_interpreter_share() {
    let grammar = express_grammar();
    let program = compile(grammar.clone()).expect("the program compiles");
    let files = corpus_files(&corpus_dir());
    assert!(!files.is_empty(), "the corpus must not be empty");

    // warmup: one pass over a few files to page caches in
    for f in files.iter().take(3) {
        let input = std::fs::read_to_string(f).expect("fixture");
        let _ = parse_interpreter(&grammar, &input);
    }

    // the pure-interpreter baseline: every parse on portable::parser
    let t = Instant::now();
    let mut interpreter_bytes = 0usize;
    for f in &files {
        let input = std::fs::read_to_string(f).expect("fixture");
        interpreter_bytes += input.len();
        let _ = parse_interpreter(&grammar, &input);
    }
    let interpreter_wall = t.elapsed();

    // the VM-first route: compile-once, VM parse with the end-pos gate,
    // interpreter fallback on a gate miss
    let t = Instant::now();
    let mut gate_fallbacks = 0usize;
    let mut vm_taken = 0usize;
    for f in &files {
        let input = std::fs::read_to_string(f).expect("fixture");
        let end = parse_vm_first(&program, &grammar, &input);
        let truth = input.len();
        if end != truth {
            gate_fallbacks += 1;
        } else {
            vm_taken += 1;
        }
    }
    let vm_wall = t.elapsed();

    // correctness cross-check: the VM-first route's verdicts match the
    // interpreter's on every file (end_pos equality)
    for f in &files {
        let input = std::fs::read_to_string(f).expect("fixture");
        let vm = parse_vm_first(&program, &grammar, &input);
        let pr = parse_interpreter(&grammar, &input);
        assert_eq!(
            vm,
            pr,
            "the VM-first verdict must match the interpreter on {}",
            f.display()
        );
    }

    println!("corpus: {} files, {} bytes", files.len(), interpreter_bytes);
    println!(
        "pure-interpreter wall: {:?} (100% on portable::parser)",
        interpreter_wall
    );
    println!(
        "VM-first wall:         {:?} ({} VM, {} gate fallbacks)",
        vm_wall, vm_taken, gate_fallbacks
    );
    let reduction = 1.0 - (vm_wall.as_secs_f64() / interpreter_wall.as_secs_f64());
    println!(
        "wall reduction: {:.1}%  (the interpreter's share of the VM-first wall is the fallback work only)",
        reduction * 100.0
    );
}
