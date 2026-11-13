//! rs#486: the per-parse fixed cost on tiny inputs. The asciichem
//! campaign measured setup ~12 µs + eval ~37 µs on chemistry lines;
//! this instrument reproduces the shape on a real artifact-sized
//! grammar (the iso corpus grammar, tiny input) and splits
//! construction from evaluation so each lever has its own leg.
//!
//! Requires PARG_ARTIFACT_DIR (the baked pubid-grammar artifacts);
//! the bench skips without it, like the artifact-gated tests.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use std::path::PathBuf;

fn artifacts_dir() -> Option<PathBuf> {
    if let Ok(from_env) = std::env::var("PARG_ARTIFACT_DIR") {
        return Some(PathBuf::from(from_env));
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest
        .parent()?
        .join("pubid")
        .join("pubid-grammar")
        .join("artifacts");
    candidate.is_dir().then_some(candidate)
}

fn bench_fixed_cost(c: &mut Criterion) {
    let Some(dir) = artifacts_dir() else {
        eprintln!("skipping: baked artifacts not present (set PARG_ARTIFACT_DIR)");
        return;
    };
    let text = std::fs::read_to_string(dir.join("iso.json")).expect("the iso artifact");
    let artifact = parsanol::PargArtifact::from_json(&text).expect("artifact loads");
    let grammar = artifact
        .grammar("identifier")
        .expect("the identifier entry");
    let input = "ISO 8601-1:2019";

    let mut group = c.benchmark_group("fixed_cost");
    group.sample_size(200);

    group.bench_function("construct_tiny_input", |b| {
        b.iter(|| {
            let mut arena =
                parsanol::portable::AstArena::for_input(black_box(input).len().max(1 << 16));
            let parser = parsanol::portable::PortableParser::new(
                black_box(&grammar),
                black_box(input),
                &mut arena,
            );
            black_box(&parser);
        })
    });

    group.bench_function("parse_and_bind_tiny_input", |b| {
        b.iter(|| black_box(artifact.parse_and_bind("identifier", black_box(input)).ok()))
    });

    group.finish();
}

criterion_group!(benches, bench_fixed_cost);
criterion_main!(benches);
