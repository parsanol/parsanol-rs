//! specialization decision (operand-specialized dispatch) comes from
//! this distribution, not intuition.

use parsanol::portable::arena::AstArena;
use parsanol::portable::grammar::Atom;
use parsanol::portable::parser::PortableParser;
use parsanol::PargArtifact;
use std::path::PathBuf;
use std::time::Instant;

fn artifact() -> PargArtifact {
    let dir = std::env::var("PARG_ARTIFACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            manifest
                .parent()
                .unwrap()
                .join("pubid")
                .join("pubid-grammar")
                .join("artifacts")
        });
    let path = dir.join("iso.json");
    let text = std::fs::read_to_string(&path).expect("the iso artifact");
    PargArtifact::from_json(&text).expect("the artifact verifies")
}

fn kind(grammar: &parsanol::portable::grammar::Grammar, id: usize) -> &'static str {
    match grammar.get_atom(id) {
        Some(Atom::Str { .. }) => "Str",
        Some(Atom::Re { .. }) => "Re",
        Some(Atom::Sequence { .. }) => "Sequence",
        Some(Atom::Alternative { .. }) => "Alternative",
        Some(Atom::Repetition { .. }) => "Repetition",
        Some(Atom::Named { .. }) => "Named",
        Some(Atom::Entity { .. }) => "Entity",
        Some(Atom::Lookahead { .. }) => "Lookahead",
        Some(Atom::Cut) => "Cut",
        Some(Atom::Ignore { .. }) => "Ignore",
        Some(Atom::Trivia { .. }) => "Trivia",
        Some(Atom::TriviaCapture { .. }) => "TriviaCapture",
        _ => "other",
    }
}

#[test]
#[ignore = "run explicitly: cargo test -p parsanol --test jit_profile -- --ignored --nocapture"]
fn profile_the_iso_grammar_dispatch_mix() {
    let artifact = artifact();
    let grammar = artifact.grammar("identifier").expect("the main entry");
    let mut arena = AstArena::new();

    // REAL inputs: the artifact's embedded test accepts, cycled to a
    // batch of 200 — the mix reflects documents the grammar accepts.
    let dir = std::env::var("PARG_ARTIFACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            manifest
                .parent()
                .unwrap()
                .join("pubid")
                .join("pubid-grammar")
                .join("artifacts")
        });
    let envelope: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("iso.json")).expect("the artifact text"),
    )
    .expect("the artifact is JSON");
    let base: Vec<String> = envelope
        .get("tests")
        .and_then(|tests| tests.as_array())
        .map(|tests| {
            tests
                .iter()
                .filter(|t| t.get("kind").and_then(|k| k.as_str()) == Some("accept"))
                .filter_map(|t| t.get("input").and_then(|i| i.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert!(!base.is_empty(), "the iso artifact ships accept inputs");
    let inputs: Vec<String> = (0..200).map(|i| base[i % base.len()].clone()).collect();

    // One identifier per parse (the entry consumes all); aggregate the
    // dispatch counts across the batch and take min-of-3 wall time over
    // the whole batch.
    let mut totals: Vec<u64> = vec![0; grammar.atom_count()];
    let mut best = None;
    for _ in 0..3 {
        let start = Instant::now();
        for input in &inputs {
            let mut parser = PortableParser::new(&grammar, input, &mut arena);
            parser.enable_profiling();
            let result = parser.parse();
            assert!(result.is_ok(), "the corpus input parses: {input}");
            for (id, count) in parser.profile_summary() {
                totals[id] += count;
            }
        }
        let elapsed = start.elapsed();
        best = Some(best.map_or(elapsed, |b: std::time::Duration| b.min(elapsed)));
    }
    let best = best.expect("three runs");

    let summary: Vec<(usize, u64)> = totals
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, c)| *c > 0)
        .collect();
    let total: u64 = summary.iter().map(|(_, c)| c).sum();
    println!("inputs: {}", inputs.len());
    println!("best-of-3 wall: {best:?}");
    println!("total dispatches: {total}");
    println!("\ntop atoms (id, kind, count, share):");
    for (id, count) in summary.iter().take(15) {
        println!(
            "  {id:>6} {:>12} {count:>10} {:>5.1}%",
            kind(&grammar, *id),
            100.0 * (*count as f64) / (total as f64)
        );
    }
    let mut by_kind: std::collections::BTreeMap<&str, u64> = Default::default();
    for (id, count) in &summary {
        *by_kind.entry(kind(&grammar, *id)).or_default() += count;
    }
    println!("\nby kind:");
    for (k, c) in &by_kind {
        println!(
            "  {k:>12} {c:>10} {:>5.1}%",
            100.0 * (*c as f64) / (total as f64)
        );
    }
}
