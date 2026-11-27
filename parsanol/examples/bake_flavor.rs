//! The flavor A/B harness: parse a pubid-grammar corpus cases-file's
//! accept inputs through the artifact, min-of-N total time, plus an
//! identity checksum over the shape JSON (the arms must agree
//! bit-for-bit).
//!
//!   cargo run --release --example bake_flavor -- <artifact.json> <entry> <cases.json> <rounds>

use std::hash::Hasher;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (artifact_path, entry, cases_path, rounds) = match args.len() {
        5 => (
            args[1].clone(),
            args[2].clone(),
            args[3].clone(),
            args[4].parse::<u32>().expect("rounds"),
        ),
        _ => panic!("usage: bake_flavor <artifact.json> <entry> <cases.json> <rounds>"),
    };
    let text = std::fs::read_to_string(&artifact_path).expect("artifact");
    let artifact = parsanol::parg::PargArtifact::from_json(&text).expect("loads");
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cases_path).expect("cases")).expect("json");
    let inputs: Vec<String> = cases["cases"]
        .as_array()
        .expect("cases array")
        .iter()
        .filter(|c| c["kind"] == "accept")
        .filter_map(|c| c["input"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(!inputs.is_empty(), "no accept cases");
    let bytes: usize = inputs.iter().map(|s| s.len()).sum();

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

    let mut best = u64::MAX;
    for _ in 0..rounds {
        let t = Instant::now();
        for input in &inputs {
            let _ = artifact.parse_shape(&entry, input);
        }
        let ns = t.elapsed().as_nanos() as u64;
        best = best.min(ns);
    }
    println!(
        "cases {} bytes {} identity {:016x} refused {refused} best_ns {best} ns_per_byte {:.4}",
        inputs.len(),
        bytes,
        identity,
        best as f64 / bytes as f64
    );
}
