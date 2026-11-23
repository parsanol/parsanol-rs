#![cfg(test)]
//! The instruction-histogram probe (Phase 2, rs#574): the opcode
//! counts over the SOTA json corpus's largest doc, the design
//! measurement behind the tranche-9/LR-fusion slice plan. Skip-guarded
//! like the corpus gates.

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
    rows.sort_by_key(|row| std::cmp::Reverse(row.0));
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

#[test]
fn t9_string_shape_probe() {
    use crate::portable::arena::AstArena;
    use crate::portable::bytecode::{compile_bytecode, vm::VMConfig, BytecodeVM};
    use crate::portable::visitor::{walk, Visitor};
    use crate::PargArtifact;
    let Ok(text) = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../benchmarks/sota/json-artifact.json"
    )) else {
        eprintln!("skip");
        return;
    };
    struct M(String);
    impl Visitor for M {
        fn visit_input_ref(&mut self, offset: u32, length: u32, input: &str) {
            self.0.push_str(&format!(
                "in:{:?}@{};",
                &input[offset as usize..offset as usize + length as usize],
                offset
            ));
        }
        fn visit_string_ref(&mut self, pool: u32, arena: &crate::portable::arena::AstArena) {
            self.0
                .push_str(&format!("s:{:?};", arena.get_string(pool as usize)));
        }
        fn visit_array_start(&mut self, _: u32, length: u32) {
            self.0.push_str(&format!("[{length}("));
        }
        fn visit_array_end(&mut self, _: u32, _: u32) {
            self.0.push_str(")];");
        }
        fn visit_hash_start(&mut self, _: u32, _: u32) {
            self.0.push('{');
        }
        fn visit_hash_key(&mut self, key: &str) {
            self.0.push_str(&format!("{key:?}->"));
        }
        fn visit_hash_end(&mut self, _: u32, _: u32) {
            self.0.push_str("};");
        }
    }
    let artifact = PargArtifact::from_json(&text).unwrap();
    let grammar = artifact.grammar("main").unwrap();
    let program = compile_bytecode(grammar).unwrap();
    let input = "{\"a\": \"bc\"}";
    let mut arena = AstArena::for_input(input.len());
    let mut vm = BytecodeVM::new(&program, input, &mut arena, VMConfig::default());
    let r = vm.run().unwrap();
    let out = walk(&r.value, &arena, input, M(String::new())).0;
    println!("walker-shaped vm tree: {out}");
    let mut w_arena = AstArena::for_input(input.len());
    let grammar2 = artifact.grammar("main").unwrap();
    let mut parser = crate::portable::PortableParser::new(&grammar2, input, &mut w_arena);
    let w = parser.parse_with_end_pos().unwrap();
    let wout = walk(&w.value, &w_arena, input, M(String::new())).0;
    println!("walker tree: {wout}");
}
