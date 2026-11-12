//! The heavy fuzz target (rs#466): seeded mutation fuzzing over the
//! grammar surface, bounded by a wall-clock budget.
//!
//! Seeds come from the conformance corpus (the ambient sibling
//! checkout, when present) plus embedded minimal grammars, so the
//! target runs everywhere. Each case mutates a seed — byte flips,
//! truncations, bracket/operator soup, rule splices — and drives the
//! public surface: `parser::parse` and `envelope::compile_source`, and
//! for artifacts that compile, the checksum-verified load and the
//! embedded-test replay. The invariants: no panics — every rejection
//! is a structured error naming its reason.
//!
//! Budget: `PARSANOL_FUZZ_SECONDS` (default 60; the scheduled heavy
//! job sets 3600). Crash cases persist under `fuzz-crashes/` so CI can
//! upload them as artifacts; the reproduction is one command:
//! `PARSANOL_FUZZ_REPLAY=<file> cargo test -p parsanol --test fuzz_heavy`.

use parsanol::parg::frontend::{envelope, parser};
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SEED_GRAMMARS: &[&str] = &[
    r#"grammar T version "1.0.0" edition 2 {
  digit = 1*( %x30-39 )
  pair = digit "-" digit
  entry main: pair
}"#,
    r#"grammar T version "1.0.0" edition 2 {
  ## the publisher vocabulary.
  publisher = %i"iso" / %i"ieee"
  identifier = ( ( publisher ) as publisher ) ":" ( 1*( %x30-39 ) as number )
  resync identifier = 1*( %x21-2E )
  entry main: identifier
  test main {
    accept "ISO:42"
    reject "nope"
  }
}"#,
    r#"grammar T version "1.0.0" edition 2 {
  item = 1*( %x30-39 ) as n
  main = item * ("," item)
  entry main: main
  bindings main {
    n -> items[].n (integer)
  }
}"#,
];

/// One mutation operator applied to a seed line pool.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, bound: usize) -> usize {
        (self.next() >> 33) as usize % bound.max(1)
    }
}

const SOUP: &[&str] = &[
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "=",
    "/",
    "*",
    "!",
    "&",
    ",",
    ";",
    ":",
    ".",
    "->",
    "##",
    "#",
    "\"",
    "\\",
    "%x30-39",
    "%i\"iso\"",
    "\"lit\"",
    "as",
    "resync",
    "entry",
    "bindings",
    "test",
    "accept",
    "reject",
    "example",
    "main",
    "rule",
    "grammar",
    "edition",
    "2",
    "é",
    "🙂",
];

fn mutate(seed: &str, rng: &mut Rng) -> String {
    let mut bytes: Vec<char> = seed.chars().collect();
    let ops = 1 + rng.below(4);
    for _ in 0..ops {
        if bytes.is_empty() {
            break;
        }
        match rng.below(6) {
            0 => {
                // byte flip
                let at = rng.below(bytes.len());
                let c = SOUP[rng.below(SOUP.len())].chars().next().unwrap();
                bytes[at] = c;
            }
            1 => {
                // truncate
                let at = rng.below(bytes.len());
                bytes.truncate(at);
            }
            2 => {
                // splice: duplicate a span
                let at = rng.below(bytes.len());
                let len = 1 + rng.below(bytes.len() - at);
                let span: Vec<char> = bytes[at..at + len].to_vec();
                let at = rng.below(bytes.len());
                bytes.splice(at..at, span);
            }
            3 => {
                // insert operator soup
                let at = rng.below(bytes.len());
                let piece = SOUP[rng.below(SOUP.len())];
                bytes.splice(at..at, piece.chars());
            }
            4 => {
                // insert a line from another seed (rule splice)
                let grams = SEED_GRAMMARS[rng.below(SEED_GRAMMARS.len())];
                let lines: Vec<&str> = grams.lines().filter(|l| l.contains(" = ")).collect();
                if let Some(line) = lines.first().copied() {
                    let at = rng.below(bytes.len());
                    bytes.splice(at..at, line.chars());
                }
            }
            _ => {
                // delete a span
                let at = rng.below(bytes.len());
                let len = 1 + rng.below((bytes.len() - at).min(12));
                bytes.drain(at..(at + len).min(bytes.len()));
            }
        }
    }
    bytes.into_iter().collect()
}

fn seed_pool() -> Vec<String> {
    let mut pool: Vec<String> = SEED_GRAMMARS.iter().map(|s| s.to_string()).collect();
    // the ambient conformance corpus, when a sibling checkout exists
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors().skip(1) {
        let dir = ancestor
            .join("pubid")
            .join("pubid-grammar")
            .join("grammars");
        if dir.is_dir() {
            let mut found: Vec<String> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("parg"))
                .filter_map(|path| std::fs::read_to_string(path).ok())
                .take(48)
                .collect();
            pool.append(&mut found);
            break;
        }
    }
    pool
}

fn crash_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().join("fuzz-crashes")
}

fn drive(input: &str, case: usize) -> bool {
    let source = input.to_string();
    let result = std::panic::catch_unwind(move || {
        // (a) the parser: structured errors only
        if let Ok(document) = parser::parse(&source) {
            let _ = black_box(&document);
            // (b) the compiler: Ok(envelope) or a String naming the reason
            if let Ok(envelope) = envelope::compile_source(&source) {
                // (c) the artifact: checksum-verified load, embedded
                // tests replay without panicking
                if let Ok(text) = serde_json::to_string(&envelope) {
                    if let Ok(artifact) = parsanol::PargArtifact::from_json(&text) {
                        let _ = black_box(artifact.run_tests());
                    }
                }
            }
        }
    });
    if result.is_err() {
        let dir = crash_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("case-{case}.parg"));
        let _ = std::fs::write(&path, input);
        eprintln!(
            "PANIC on case {case}: input persisted at {}",
            path.display()
        );
        return false;
    }
    true
}

#[test]
fn mutation_fuzz_stays_structured_within_budget() {
    if let Ok(replay) = std::env::var("PARSANOL_FUZZ_REPLAY") {
        // cargo runs the test from the package dir; relative replay
        // paths resolve against the workspace (where crashes persist)
        let path = PathBuf::from(&replay);
        let path = if path.is_absolute() {
            path
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join(path)
        };
        let input = std::fs::read_to_string(&path).expect("replay file");
        assert!(drive(&input, 0), "the replayed case panics — see {replay}");
        return;
    }
    let seconds: u64 = std::env::var("PARSANOL_FUZZ_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let budget = Duration::from_secs(seconds);
    let pool = seed_pool();
    assert!(!pool.is_empty(), "the seed pool is never empty");
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let started = Instant::now();
    let mut case = 0usize;
    while started.elapsed() < budget {
        let seed = &pool[rng.below(pool.len())];
        let input = mutate(seed, &mut rng);
        if !drive(&input, case) {
            panic!("the fuzzer found a panic — reproduce with PARSANOL_FUZZ_REPLAY");
        }
        case += 1;
    }
    eprintln!(
        "fuzzed {case} cases in {}s, 0 panics (pool: {} seeds)",
        started.elapsed().as_secs(),
        pool.len()
    );
}
