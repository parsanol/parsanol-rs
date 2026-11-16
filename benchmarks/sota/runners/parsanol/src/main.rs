//! The parsanol runner for the industry benchmark suite (rs#526).
//!
//! Usage: run_parsanol <artifact.json> <corpus-dir> <rounds> <vm|interpreter>
//!
//! Parses every corpus document `rounds` times with the selected engine,
//! prints one line per document: `<doc> <bytes> <ns_per_parse>` (the
//! minimum over the rounds), then `TOTAL <bytes> <total_ns>`. The driver
//! aggregates the totals into the published table.
//!
//! Both engines must fully consume each document; any failure exits
//! non-zero (a bench that silently short-parses is worthless — rs#496).

use parsanol::portable::bytecode::compiler::compile;
use parsanol::portable::bytecode::parse_with_vm_diag;
use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 6 {
        eprintln!("usage: run_parsanol <artifact.json> <entry> <corpus-dir> <rounds> <vm|interpreter>");
        std::process::exit(2);
    }
    let artifact_text = std::fs::read_to_string(&args[1]).expect("artifact");
    let entry = args[2].clone();
    let corpus_dir = std::path::Path::new(&args[3]);
    let rounds: u32 = args[4].parse().expect("rounds");
    let engine = args[5].clone();
    assert!(engine == "vm" || engine == "interpreter", "engine = vm|interpreter");

    let artifact = parsanol::PargArtifact::from_json(&artifact_text).expect("artifact loads");
    let grammar = artifact.grammar(&entry).expect("the entry decodes");
    let program = compile(grammar.clone()).expect("the program compiles");

    let mut docs: Vec<(String, String)> = Vec::new();
    let mut total_bytes = 0usize;
    for entry in std::fs::read_dir(corpus_dir).expect("corpus dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|x| x == "json") {
            let input = std::fs::read_to_string(&path).expect("doc");
            total_bytes += input.len();
            docs.push((path.file_name().unwrap().to_string_lossy().into_owned(), input));
        }
    }
    docs.sort();
    assert!(!docs.is_empty(), "no corpus documents");

    // warmup: one full pass, verifying both engines fully consume
    for (_, input) in &docs {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let end = parser
            .parse_with_end_pos()
            .unwrap_or_else(|e| panic!("the interpreter must parse every doc: {e:?}"));
        assert_eq!(end.end_pos, input.len(), "the interpreter must consume the doc");
        let mut arena = AstArena::for_input(input.len());
        let (vm, _) = parse_with_vm_diag(&program, input, &mut arena);
        let vm = vm.expect("the VM must parse every doc");
        assert_eq!(vm.end_pos, input.len(), "the VM must consume the doc");
    }

    let mut total_ns = 0u128;
    for (name, input) in &docs {
        let mut best = u128::MAX;
        for _ in 0..rounds {
            let t = Instant::now();
            match engine.as_str() {
                "interpreter" => {
                    let mut arena = AstArena::for_input(input.len());
                    let mut parser = PortableParser::new(&grammar, input, &mut arena);
                    let end = black_box(parser.parse_with_end_pos()).expect("parse");
                    assert_eq!(end.end_pos, input.len());
                }
                _ => {
                    let mut arena = AstArena::for_input(input.len());
                    let (vm, _) = parse_with_vm_diag(&program, input, &mut arena);
                    let vm = black_box(vm).expect("parse");
                    assert_eq!(vm.end_pos, input.len());
                }
            }
            let ns = t.elapsed().as_nanos();
            if ns < best {
                best = ns;
            }
        }
        total_ns += best;
        println!("{name} {} {best}", input.len());
    }
    println!("TOTAL {total_bytes} {total_ns}");
}
