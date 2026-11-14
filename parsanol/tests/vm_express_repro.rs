//! rs#496: the bytecode VM silently short-parses the compiled EXPRESS
//! grammar. The repro (expressir-rs perf/507-bytecode-parse): the
//! edition-2 iso-10303-11 grammar compiled via the parg frontend
//! (~42k instructions), parsing expressir's 21 KB syntax.exp — the
//! packrat interpreter parses fully (end_pos 21028) while the VM
//! returns Ok having consumed only the 10-byte lead comment.
//!
//! #[ignore]d: needs the expressir-rs assets on disk. Run:
//!   EXPRESSIR_ASSETS=~/src/lutaml/expressir-rs/assets \
//!     cargo test -p parsanol --test vm_express_repro -- --ignored

use parsanol::portable::bytecode::compiler::compile;
use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::path::PathBuf;

fn assets_dir() -> PathBuf {
    std::env::var("EXPRESSIR_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../../lutaml/expressir-rs/assets"))
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

use std::collections::BTreeMap;

#[test]
#[ignore = "needs the expressir-rs assets - bisects syntax.exp to the shortest VM-failing prefix"]
fn bisect_the_shortest_vm_failing_prefix() {
    let grammar = express_grammar();
    let input = std::fs::read_to_string(
        std::env::var("EXPRESSIR_FIXTURE")
            .unwrap_or_else(|_| "../../lutaml/expressir-rs/tests/fixtures/syntax.exp".into()),
    )
    .expect("syntax.exp");
    let program = compile(grammar.clone()).expect("the program compiles");

    for i in 46..95 {
        println!("I{i:02} {:?}", program.get_instruction(i).unwrap());
    }

    // declaration boundaries: prefixes ending at `;` line-ends
    let mut cutpoints: Vec<usize> = input.match_indices(";\n").map(|(i, _)| i + 2).collect();
    cutpoints.push(input.len());
    let mut first_bad = None;
    for cut in cutpoints.iter() {
        let prefix = &input[..*cut];
        let mut arena = AstArena::for_input(prefix.len());
        let (vm, _) =
            parsanol::portable::bytecode::parse_with_vm_diag(&program, prefix, &mut arena);
        let vm_end = vm.as_ref().map(|r| r.end_pos).unwrap_or(usize::MAX);
        let mut packrat_arena = AstArena::for_input(prefix.len());
        let mut pr = PortableParser::new(&grammar, prefix, &mut packrat_arena);
        let pr_end = pr
            .parse_with_end_pos()
            .map(|r| r.end_pos)
            .unwrap_or(usize::MAX);
        if vm_end != pr_end {
            println!("DIVERGE at prefix len {cut}: packrat {pr_end} vm {vm_end}");
            first_bad = Some(*cut);
            break;
        }
    }
    println!(
        "first diverging prefix: {:?} of {} cutpoints",
        first_bad,
        cutpoints.len()
    );
    if let Some(cut) = first_bad {
        let tail_start = cut.saturating_sub(200);
        println!("the divergence region (last 200 bytes of the prefix):");
        println!("{}", &input[tail_start..cut]);
    }
}

#[test]
#[ignore = "needs the expressir-rs assets (EXPRESSIR_ASSETS) - the VM short-parse repro, rs#496"]
fn vm_matches_packrat_end_pos_on_the_express_corpus() {
    let grammar = express_grammar();
    let input = std::fs::read_to_string(
        std::env::var("EXPRESSIR_FIXTURE")
            .unwrap_or_else(|_| "../../lutaml/expressir-rs/tests/fixtures/syntax.exp".into()),
    )
    .expect("syntax.exp");
    let program = compile(grammar.clone()).expect("the program compiles");

    let mut packrat_arena = AstArena::for_input(input.len());
    let mut packrat = PortableParser::new(&grammar, &input, &mut packrat_arena);
    let packrat = packrat
        .parse_with_end_pos()
        .expect("packrat parses the corpus");
    println!("packrat end_pos: {}", packrat.end_pos);

    let mut vm_arena = AstArena::for_input(input.len());
    let (vm, diag) =
        parsanol::portable::bytecode::parse_with_vm_diag(&program, &input, &mut vm_arena);
    let vm = vm.expect("the VM reports Ok (the bug: a silent short parse)");
    println!("vm end_pos: {}", vm.end_pos);
    if let Some((offset, labels)) = diag {
        println!("vm deepest failure: offset {offset} expected {labels:?}");
    }

    assert_eq!(
        vm.end_pos, packrat.end_pos,
        "the VM must consume what packrat consumes (rs#496: it stops at the lead comment)"
    );
}
