//! rs#499 diagnostic: parse the minimal `s IN t` snippet with the
//! compiled EXPRESS grammar and dump the failure wire + deepest-failure
//! diagnostics, so the mis-injection is observed rather than inferred.

use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn express_grammar() -> Grammar {
    let dir = std::env::var("EXPRESSIR_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../../lutaml/expressir-rs/assets"));
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

#[test]
#[ignore = "diagnostic harness - run explicitly with EXPRESSIR_ASSETS set"]
fn dump_the_tin_atoms() {
    let grammar = express_grammar();
    for (i, atom) in grammar.atoms.iter().enumerate().take(20000) {
        let text = format!("{atom:?}");
        if i <= 12 || (2650..=2675).contains(&i) {
            println!("{i}: {text}");
        }
    }
}

#[test]
#[ignore = "diagnostic harness - run explicitly with EXPRESSIR_ASSETS set"]
fn dump_the_in_failure() {
    let grammar = express_grammar();
    let input = "SCHEMA explicit_constraint_schema;\n\
ENTITY e;\n\
WHERE\n\
  wr1: s IN t;\n\
END_ENTITY;\n\
END_SCHEMA;\n";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let result = parser.parse_with_end_pos();
    match &result {
        Ok(r) => println!("parse Ok end_pos={}", r.end_pos),
        Err(e) => println!("parse Err: {e}"),
    }
    if let Some((offset, labels)) = parser.failure_diagnostics() {
        println!(
            "deepest offset={offset} input={:?} expected={labels:?}",
            &input[offset..(offset + 8).min(input.len())]
        );
    }
    let ranked = parser.failure_ranks();
    for (offset, labels) in ranked.iter().take(6) {
        println!(
            "ranked offset={offset} {:?} expected={:?}",
            &input[*offset..(*offset + 6).min(input.len())],
            labels
        );
    }
}
