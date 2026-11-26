//! The P3(a) A/B harness (PARG_LR_SHIFT): parses a flavor's corpus
//! inputs through its artifact with the gate off/on and reports the
//! total parse time plus a tree checksum — the two arms' checksums
//! must match for the byte-identical differential to hold on this
//! workload. Run as two processes (the env is read per compile):
//!
//!   cargo run --release --example ab_shift -- <artifact.json> <entry> <inputs-file> off|on <rounds>

use std::hash::Hasher;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (artifact_path, entry, inputs_path, arm, rounds) = match args.len() {
        6 => (
            args[1].clone(),
            args[2].clone(),
            args[3].clone(),
            args[4].clone(),
            args[5].parse::<u32>().expect("rounds"),
        ),
        _ => panic!("usage: ab_shift <artifact.json> <entry> <inputs-file> off|on <rounds>"),
    };
    assert!(arm == "off" || arm == "on");
    if arm == "on" {
        std::env::set_var("PARG_LR_SHIFT", "1");
    }
    let text = std::fs::read_to_string(&artifact_path).expect("artifact");
    let artifact = parsanol::parg::PargArtifact::from_json(&text).expect("loads");
    let inputs: Vec<String> = std::fs::read_to_string(&inputs_path)
        .expect("inputs")
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.is_empty())
        .collect();
    assert!(!inputs.is_empty(), "no inputs");

    // The identity checksum: the shape JSON of every parse, hashed in
    // order, one canonical pass. The arms must agree bit-for-bit.
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    let mut refused = 0usize;
    for input in &inputs {
        match artifact.parse_shape(&entry, input) {
            Ok(shape) => {
                let json = serde_json::to_string(&shape).expect("shape serializes");
                identity.write(json.as_bytes());
            }
            Err(e) => {
                refused += 1;
                identity.write(format!("ERR {e}").as_bytes());
            }
        }
        identity.write(b"\x00");
    }
    let identity = identity.finish();

    // The timed rounds: the whole input set per round, min-of-N.
    let mut best = u64::MAX;
    for _ in 0..rounds {
        let t = Instant::now();
        for input in &inputs {
            let _ = artifact.parse_shape(&entry, input);
        }
        best = best.min(t.elapsed().as_nanos() as u64);
    }
    println!(
        "{arm} best_ns {best} inputs {} refused {refused} identity {identity:016x}",
        inputs.len()
    );
}
