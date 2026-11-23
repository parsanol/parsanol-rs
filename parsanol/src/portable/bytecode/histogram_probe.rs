#![cfg(test)]

#[test]
fn histogram_probe_json() {
    use crate::portable::arena::AstArena;
    use crate::portable::bytecode::vm::BytecodeVM;
    use crate::portable::bytecode::{compile_bytecode, vm::VMConfig, Opcode};
    use crate::PargArtifact;
    let Ok(text) = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../benchmarks/sota/json-artifact.json"
    )) else {
        eprintln!("skip: no sibling benchmarks/sota checkout");
        return;
    };
    let artifact = PargArtifact::from_json(&text).unwrap();
    let grammar = artifact.grammar("main").unwrap();
    let program = compile_bytecode(grammar.clone()).unwrap();
    println!("program instructions: {}", program.instruction_count());
    let input = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../benchmarks/sota/corpora/json/doc3.json"
    ))
    .unwrap();
    let mut arena = AstArena::for_input(input.len());
    let mut vm = BytecodeVM::new(&program, &input, &mut arena, VMConfig::default());
    vm.enable_profiling();
    let result = vm.run();
    assert!(result.is_ok());
    let counts = vm.opcode_counts().unwrap();
    // The program's own instructions define the discriminant -> name map.
    let mut names: std::collections::HashMap<u8, String> = std::collections::HashMap::new();
    for i in 0..program.instruction_count() {
        if let Some(instr) = program.get_instruction(i) {
            let op: Opcode = instr.opcode();
            names.insert(op as u8, format!("{op:?}"));
        }
    }
    let mut rows: Vec<(u64, String)> = Vec::new();
    let mut total = 0u64;
    for (i, &c) in counts.iter().enumerate() {
        if c > 0 {
            total += c;
            if let Some(name) = names.get(&(i as u8)) {
                rows.push((c, name.clone()));
            } else {
                rows.push((c, format!("op{i}")));
            }
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    println!("total dispatched: {total}");
    for (c, name) in rows.iter().take(18) {
        println!(
            "{:8} {:6.2}%  {}",
            c,
            (*c as f64) * 100.0 / total as f64,
            name
        );
    }
}
