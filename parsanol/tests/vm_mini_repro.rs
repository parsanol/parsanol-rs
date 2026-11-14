//! rs#496 probe: the minimal ABSTRACT;-in-subtype-constraint repro.
//!
//! Run: EXPRESSIR_ASSETS=~/src/lutaml/expressir-rs/assets \
//!   cargo test -p parsanol --test vm_mini_repro -- --ignored --nocapture

use parsanol::portable::bytecode::compiler::compile;
use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::collections::BTreeMap;
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

const HEAD: &str = "SCHEMA syntax_schema '{ISO standard 10303 part(41) object(1) version(8)}';\nENTITY empty_entity;\nEND_ENTITY;\n";
const TAIL: &str = "END_SCHEMA;\n";
const PLAIN: &str =
    "SUBTYPE_CONSTRAINT empty_subtype_constraint FOR empty_entity; END_SUBTYPE_CONSTRAINT;\n";
const ABSTRACT: &str = "SUBTYPE_CONSTRAINT abstract_supertype_subtype_constraint FOR empty_entity; ABSTRACT; END_SUBTYPE_CONSTRAINT;\n";

#[test]
#[ignore = "needs the expressir-rs assets - the minimal ABSTRACT; probe"]
fn vm_matches_packrat_on_the_mini_abstract_repro() {
    let grammar = express_grammar();
    let program = compile(grammar.clone()).expect("the program compiles");

    let variants: [(&str, String); 4] = [
        ("mini", format!("{HEAD}{PLAIN}{ABSTRACT}{TAIL}")),
        ("abstract_only", format!("{HEAD}{ABSTRACT}{TAIL}")),
        ("plain_only", format!("{HEAD}{PLAIN}{TAIL}")),
        ("abstract_supertype", format!("{HEAD}{PLAIN}SUBTYPE_CONSTRAINT x FOR empty_entity; ABSTRACT SUPERTYPE; END_SUBTYPE_CONSTRAINT;\n{TAIL}")),
    ];
    let run_variants = std::env::var("PARSANOL_SKIP_VARIANTS").is_err();
    if run_variants {
        for (name, input) in variants {
            let mut packrat_arena = AstArena::for_input(input.len());
            let mut packrat = PortableParser::new(&grammar, &input, &mut packrat_arena);
            let pr = packrat.parse_with_end_pos();
            let mut vm_arena = AstArena::for_input(input.len());
            let (vm, diag) =
                parsanol::portable::bytecode::parse_with_vm_diag(&program, &input, &mut vm_arena);
            println!(
                "{name}: packrat {:?} vm {:?} deepest {:?}",
                pr.as_ref().map(|r| r.end_pos).map_err(|e| e.to_string()),
                vm.as_ref().map(|r| r.end_pos).map_err(|e| e.to_string()),
                diag.map(|(o, l)| (o, l.len()))
            );
        }
    }
    // the assertion keeps the case that matters red until fixed
    let input = format!("{HEAD}{ABSTRACT}{TAIL}");
    let mut packrat_arena = AstArena::for_input(input.len());
    let mut packrat = PortableParser::new(&grammar, &input, &mut packrat_arena);
    let pr_end = packrat
        .parse_with_end_pos()
        .expect("packrat parses")
        .end_pos;
    let mut vm_arena = AstArena::for_input(input.len());
    let (vm, _) = parsanol::portable::bytecode::parse_with_vm_diag(&program, &input, &mut vm_arena);
    assert_eq!(
        vm.expect("vm ok").end_pos,
        pr_end,
        "the VM must fully parse the abstract-only fixture"
    );
}
