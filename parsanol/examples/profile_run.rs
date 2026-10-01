//! Profiling harness for the artifact hot path (dev-only, not published).
//! The #115 quoted-string shape: repetition of (escape | run-as) inside a
//! literal pair, where each run iteration is a per-char capture.

use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::time::Instant;

const GRAMMAR: &str = r#"{"atoms":[{"Str":{"pattern":"\""}},{"Entity":{"atom":17}},{"Str":{"pattern":"\\"}},{"Str":{"pattern":"\""}},{"Named":{"name":"dquote","atom":3}},{"Str":{"pattern":"n"}},{"Named":{"name":"newline","atom":5}},{"Alternative":{"atoms":[4,6]}},{"Sequence":{"atoms":[2,7]}},{"Entity":{"atom":17}},{"Str":{"pattern":"\\"}},{"Str":{"pattern":"\""}},{"Alternative":{"atoms":[10,11]}},{"Lookahead":{"atom":12,"positive":false}},{"Entity":{"atom":15}},{"Re":{"pattern":"[\\x00-\\u{10ffff}]"}},{"Sequence":{"atoms":[13,14]}},{"Repetition":{"atom":16,"min":1,"max":null,"tag":"Repetition"}},{"Named":{"name":"run","atom":9}},{"Alternative":{"atoms":[1,18]}},{"Repetition":{"atom":19,"min":0,"max":null,"tag":"Repetition"}},{"Named":{"name":"string","atom":20}},{"Str":{"pattern":"\""}},{"Sequence":{"atoms":[0,21,22]}}],"root":23}"#;

fn main() {
    let grammar = Grammar::from_json(GRAMMAR).unwrap();
    let input = format!("\"{}\"", "x".repeat(50_000));

    // warmup
    for _ in 0..3 {
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.clone());
        let mut p = PortableParser::new(&grammar, &input, &mut arena);
        let _ = p.parse().unwrap();
    }

    let n = 20;
    let t0 = Instant::now();
    for _ in 0..n {
        let mut arena = AstArena::for_input(input.len());
        arena.set_input(input.clone());
        let mut p = PortableParser::new(&grammar, &input, &mut arena);
        let _ = p.parse().unwrap();
    }
    let dt = t0.elapsed();
    println!(
        "{n} parses of 50KB in {dt:?} — {:.2} ms/parse, {:.0} ns/char",
        dt.as_millis() as f64 / n as f64,
        dt.as_nanos() as f64 / (n as f64 * 50_000.0)
    );
}
