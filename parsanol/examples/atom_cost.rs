//! Per-char cost probe for a grammar entry in a PARG artifact JSON.
//!   cargo run --release -p parsanol --example atom_cost -- <artifact.json> <entry> (<unit> <n> | FILE <path>)
//! With `--features perf-counters` it also prints the engine's event counters.
use parsanol::portable::arena::AstArena;
use parsanol::portable::grammar::Grammar;
use parsanol::portable::parser::PortableParser;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let g = &doc["entries"][a[2].as_str()]["grammar"];
    let grammar = Grammar::from_json(&g.to_string()).unwrap();
    // `FILE <path>` parses a file once; otherwise `<unit> <n>` repeats a unit.
    let (input, n) = if a[3] == "FILE" {
        (std::fs::read_to_string(&a[4]).unwrap(), 1)
    } else {
        let n: usize = a[4].parse().unwrap();
        (a[3].repeat(n), n)
    };
    let mut arena = AstArena::new();
    arena.set_input(input.clone());
    let mut parser = PortableParser::new(&grammar, &input, &mut arena);
    let t = Instant::now();
    let r = parser.parse();
    let dt = t.elapsed();
    println!(
        "ok={} {:?} {:.3} us/unit",
        r.is_ok(),
        dt,
        dt.as_secs_f64() * 1e6 / n as f64
    );
    #[cfg(feature = "perf-counters")]
    println!("{}", parser.perf_counters_report());
}
