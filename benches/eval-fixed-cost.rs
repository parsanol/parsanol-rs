//! rs#486: the per-parse fixed cost on tiny inputs. The asciichem
//! campaign measured setup ~12 µs + eval ~37 µs on chemistry lines;
//! this instrument reproduces the shape on a real artifact-sized
//! grammar (the iso corpus grammar, tiny input) and splits
//! construction from evaluation so each lever has its own leg.
//!
//! Requires PARG_ARTIFACT_DIR (the baked pubid-grammar artifacts);
//! the bench skips without it, like the artifact-gated tests.
//!
//! The `asciichem_*` group is the campaign's own shape: the shared
//! 10-input workload (identical to asciichem-rs benches/parse.rs and
//! the TS bench) parsed through the formula entry, split into the
//! same legs — `construct` (arena + parser), `parse` (the eval loop,
//! via parse_with_end_pos), `plumbing` (the parslet transform +
//! value conversion asciichem runs after the parse). Requires
//! ASCIICHEM_ARTIFACT pointing at the baked asciichem.json; the
//! group skips without it.

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

fn asciichem_artifact() -> Option<PathBuf> {
    std::env::var("ASCIICHEM_ARTIFACT").ok().map(PathBuf::from)
}

/// The shared cross-runtime workload (identical to the asciichem-rs
/// bench and the TS bench contract).
const INPUTS: [&str; 10] = [
    "H_2O",
    "Ca^2+",
    "SO_4^2-",
    "(R)-CH_3CH(OH)COOH",
    "2H_2 + O_2 -> 2H_2O",
    "N_2 + 3H_2 <=>[Fe][400C] 2NH_3",
    "C1-C-C-C-C-C1",
    "CH_3-CH_2-OH",
    "^14C @name(\"carbon-14\") @cas(\"14104-86-4\")",
    "A ->[heat] B ->[cool] C",
];

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

    // The asciichem campaign's shape: the formula entry over the shared
    // workload, construct / eval / plumbing split. asciichem parses with
    // PortableParser::new over AstArena::for_input(source.len()) — no
    // tree-sized floor — then transforms; every leg below mirrors that
    // sequence exactly.
    if let Some(path) = asciichem_artifact() {
        let text = std::fs::read_to_string(&path).expect("the asciichem artifact");
        let artifact = parsanol::PargArtifact::from_json(&text).expect("artifact loads");
        let grammar = artifact.grammar("formula").expect("the formula entry");

        let mut group = c.benchmark_group("asciichem_fixed_cost");
        group.sample_size(200);

        // Per-input legs on the tiny end: the fixed cost lives here.
        for input in ["H_2O", "Ca^2+", "SO_4^2-"] {
            group.bench_function(format!("construct/{input}"), |b| {
                b.iter(|| {
                    let mut arena = parsanol::portable::AstArena::for_input(black_box(input).len());
                    let parser = parsanol::portable::PortableParser::new(
                        black_box(&grammar),
                        black_box(input),
                        &mut arena,
                    );
                    black_box(&parser);
                })
            });

            group.bench_function(format!("parse/{input}"), |b| {
                b.iter(|| {
                    let mut arena = parsanol::portable::AstArena::for_input(black_box(input).len());
                    let mut parser = parsanol::portable::PortableParser::new(
                        black_box(&grammar),
                        black_box(input),
                        &mut arena,
                    );
                    black_box(parser.parse_with_end_pos().ok())
                })
            });

            group.bench_function(format!("plumbing/{input}"), |b| {
                b.iter(|| {
                    let mut arena = parsanol::portable::AstArena::for_input(black_box(input).len());
                    let mut parser = parsanol::portable::PortableParser::new(
                        black_box(&grammar),
                        black_box(input),
                        &mut arena,
                    );
                    let result = parser.parse_with_end_pos().ok();
                    let source = black_box(input);
                    result.map(|r| {
                        let shaped = parsanol::portable::parslet_transform::to_parslet_compatible(
                            &r.value, &mut arena, source,
                        );
                        black_box(parsanol::portable::transform::ast_to_value(
                            &shaped, &arena, source,
                        ))
                    })
                })
            });
        }

        // The shared workload end to end (one pass): comparable to the
        // asciichem-rs bench's per-parse numbers.
        group.bench_function("parse x10 inputs", |b| {
            b.iter(|| {
                for input in INPUTS {
                    let mut arena = parsanol::portable::AstArena::for_input(black_box(input).len());
                    let mut parser = parsanol::portable::PortableParser::new(
                        black_box(&grammar),
                        black_box(input),
                        &mut arena,
                    );
                    black_box(parser.parse_with_end_pos().ok());
                }
            })
        });

        group.finish();
    } else {
        eprintln!("skipping asciichem_fixed_cost: set ASCIICHEM_ARTIFACT");
    }
}

criterion_group!(benches, bench_fixed_cost);
criterion_main!(benches);
