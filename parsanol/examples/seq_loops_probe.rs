fn main() {
    let source = std::fs::read_to_string("/tmp/p103/seq_loops.parg").unwrap();
    let document = parsanol::parg::frontend::parser::parse(&source).expect("parses");
    let grammar = parsanol::parg::frontend::compile::compile_entry(
        &document,
        "doc",
        &parsanol::parg::frontend::compile::Tables::default(),
    )
    .expect("compiles");
    let program = parsanol::portable::bytecode::compiler::compile(grammar).expect("compiles");
    let mut kinds: std::collections::BTreeMap<&'static str, usize> = Default::default();
    for ins in program.instructions() {
        let name = match ins {
            parsanol::portable::bytecode::instruction::Instruction::DfaRun { .. } => "DfaRun",
            parsanol::portable::bytecode::instruction::Instruction::ScanClass { .. } => "ScanClass",
            parsanol::portable::bytecode::instruction::Instruction::BuildRep { .. } => "BuildRep",
            parsanol::portable::bytecode::instruction::Instruction::LrRun { .. } => "LrRun",
            _ => "other",
        };
        *kinds.entry(name).or_default() += 1;
    }
    println!("instruction mix: {:?}", kinds);
}
