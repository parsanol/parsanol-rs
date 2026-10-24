//! Recovery quality metrics (rs#304): a seeded, deterministic harness
//! over the conformance corpus's recovery artifacts. Each accept-case
//! input is corrupted with a fixed LCG corruption model (delete /
//! substitute / insert / duplicate), then parsed with recovery; the
//! harness reports the recoverable fraction, precision proxy (how
//! much of the clean capture set the recovered parse still commits),
//! and bytes-to-recovery (failure offset to the first resync).
//!
//! The published table comes from this harness; the determinism
//! assertion (same seed, same numbers) is the regression gate's
//! foundation — recovery behavior changes show up as metric moves,
//! never as noise.

use parsanol::PargArtifact;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("corpus")
        .join("conformance")
}

const RECOVERY_ARTIFACTS: [&str; 2] = ["recovery_resync_declared", "recovery_kv"];

/// A fixed 64-bit LCG: the corruption stream is a pure function of the
/// seed, so every engine and every run corrupts identically.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Delete,
    Substitute,
    Insert,
    Duplicate,
}

const KINDS: [Kind; 4] = [Kind::Delete, Kind::Substitute, Kind::Insert, Kind::Duplicate];

fn corrupt(input: &str, seed: u64, count: usize) -> String {
    let mut rng = Lcg(seed);
    let mut bytes: Vec<u8> = input.as_bytes().to_vec();
    for _ in 0..count {
        if bytes.is_empty() {
            bytes.push(b'x');
            continue;
        }
        let kind = KINDS[(rng.next() % 4) as usize];
        // Byte positions only on the ASCII prefix of these corpus
        // inputs; the artifacts' inputs are ASCII, so byte edits stay
        // UTF-8-valid. Guard anyway: never split a multibyte lead.
        let mut pos = (rng.next() as usize) % bytes.len();
        while bytes[pos] & 0xC0 == 0x80 && pos > 0 {
            pos -= 1;
        }
        match kind {
            Kind::Delete => {
                bytes.remove(pos);
            }
            Kind::Substitute => {
                bytes[pos] = b'z';
            }
            Kind::Insert => {
                bytes.insert(pos, b'z');
            }
            Kind::Duplicate => {
                bytes.insert(pos, bytes[pos]);
            }
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| input.to_string())
}

struct Metrics {
    recoverable: usize,
    total: usize,
    precision_sum: f64,
    precision_n: usize,
    bytes_to_recovery: Vec<usize>,
}

fn run_case(artifact: &PargArtifact, entry: &str, clean: &str, seeds: u64) -> Metrics {
    let clean_shape = artifact.parse_shape(entry, clean).ok();
    let clean_nodes = capture_nodes(clean_shape.as_ref());
    let mut metrics = Metrics {
        recoverable: 0,
        total: 0,
        precision_sum: 0.0,
        precision_n: 0,
        bytes_to_recovery: Vec::new(),
    };
    for seed in 0..seeds {
        let corrupted = corrupt(clean, 0x5EED_0000 + seed, 1 + (seed % 3) as usize);
        metrics.total += 1;
        let Ok(envelope) = artifact.parse_shape_recovering(entry, &corrupted) else {
            continue;
        };
        if envelope["ok"].as_bool() == Some(true) {
            // The corruption did not even break the parse: recovery
            // was unnecessary — count as recovered with full fidelity.
            metrics.recoverable += 1;
            metrics.precision_sum += 1.0;
            metrics.precision_n += 1;
            continue;
        }
        let resyncs = envelope["resyncs"].as_array();
        if let Some(first) = resyncs.and_then(|spans| spans.first()) {
            metrics.recoverable += 1;
            if let (Some(start), Some(failure)) =
                (first["offset"].as_u64(), envelope["error"]["offset"].as_u64())
            {
                // The resync span can start before the reported deepest failure
                // position (the wire reports the deepest, the policy may
                // resume from an earlier spine iteration).
                metrics
                    .bytes_to_recovery
                    .push(start.saturating_sub(failure) as usize);
            }
        }
        let recovered_nodes = capture_nodes(envelope.get("captures"));
        if !clean_nodes.is_empty() {
            let kept = clean_nodes
                .iter()
                .filter(|node| recovered_nodes.contains(node))
                .count();
            metrics.precision_sum += kept as f64 / clean_nodes.len() as f64;
            metrics.precision_n += 1;
        }
    }
    metrics
}

/// (capture-name, value-text) pairs — the NODE comparison for
/// precision: a recovered node matches when the clean document has
/// the same capture carrying the same text. Positions are ignored
/// (corruption moves everything after the failure point).
fn capture_nodes(value: Option<&Value>) -> Vec<(String, String)> {
    let mut nodes = Vec::new();
    let Some(root) = value else {
        return nodes;
    };
    let mut queue = vec![root];
    while let Some(node) = queue.pop() {
        match node {
            Value::Object(map) => {
                for (key, child) in map {
                    if !matches!(key.as_str(), "offset" | "length" | "line" | "column") {
                        if let Some(text) = child.as_str() {
                            nodes.push((key.clone(), text.to_string()));
                        }
                    }
                    queue.push(child);
                }
            }
            Value::Array(items) => queue.extend(items.iter()),
            _ => {}
        }
    }
    nodes.sort();
    nodes.dedup();
    nodes
}

#[test]
fn recovery_metrics_are_deterministic_and_resync_recovers() {
    let dir = conformance_dir();
    if !dir.is_dir() {
        eprintln!("skipping: corpus/conformance not present outside the workspace");
        return;
    }

    let mut lines = Vec::new();
    for name in RECOVERY_ARTIFACTS {
        let path = dir.join(format!("{name}.json"));
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let envelope: Value = serde_json::from_str(&text).expect("artifact json");
        let artifact = PargArtifact::from_json(&text).expect("artifact loads");
        let entry = envelope["default_entry"].as_str().expect("entry").to_string();
        let clean = envelope["tests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|test| test["kind"] == "accept")
            .and_then(|test| test["input"].as_str())
            .expect("accept case")
            .to_string();

        let first = run_case(&artifact, &entry, &clean, 64);
        let second = run_case(&artifact, &entry, &clean, 64);
        assert_eq!(
            first.total, second.total,
            "{name}: corruption count must be deterministic"
        );
        assert_eq!(
            first.recoverable, second.recoverable,
            "{name}: recovery outcomes must be deterministic for a fixed seed stream"
        );
        assert_eq!(
            first.bytes_to_recovery, second.bytes_to_recovery,
            "{name}: bytes-to-recovery must be deterministic"
        );

        let fraction = first.recoverable as f64 / first.total as f64;
        let precision = if first.precision_n > 0 {
            first.precision_sum / first.precision_n as f64
        } else {
            0.0
        };
        let median = {
            let mut spans = first.bytes_to_recovery.clone();
            spans.sort_unstable();
            spans.get(spans.len() / 2).copied().unwrap_or(0)
        };
        lines.push(format!(
            "{name}: recoverable {}/{} ({:.0}%), precision {:.2}, bytes-to-recovery median {}",
            first.recoverable, first.total, fraction * 100.0, precision, median
        ));
        // The declared-resync artifact declares a policy; corruption
        // must never leave it with zero recoveries.
        if name == "recovery_resync_declared" {
            assert!(
                first.recoverable > 0,
                "{name}: a declared resync policy recovered nothing across 64 seeds"
            );
        }
    }
    assert!(!lines.is_empty(), "no recovery artifacts measured");
    println!("recovery quality (seeded, deterministic):");
    for line in lines {
        println!("  {line}");
    }
}
