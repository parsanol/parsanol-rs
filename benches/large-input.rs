//! Large-input memoization benchmark (parsanol-ruby#52)
//!
//! Reproduces the super-linear cost on large inputs reported in
//! parsanol-ruby#52: with only Alternative/Repetition memoized, sequences
//! and named rules were re-executed from every structural path, so profiles
//! showed ~99% of samples in try_atom/parse_atom_uncached with almost no
//! cache traffic. The grammar deliberately round-trips through JSON
//! (`Grammar::from_json`) because that is the registration path used by the
//! Ruby FFI, and the only path where per-atom caching policy is computed.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use parsanol::portable::parser_dsl::{
    capture, choice, dynamic, re, ref_, seq, str, GrammarBuilder, ParsletExt,
};
use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::hint::black_box;

fn grammar_json() -> String {
    let value = choice(vec![
        // Shared-prefix alternative: `ident` is visited at the same position
        // by two structural paths, which is what memoization must absorb.
        dynamic(seq(vec![
            dynamic(ref_("ident")),
            dynamic(str(".")),
            dynamic(ref_("ident")),
        ])),
        dynamic(ref_("ident")),
        dynamic(ref_("number")),
    ]);
    let line = seq(vec![
        dynamic(ref_("ident")),
        dynamic(re(r"\s+").ignore()),
        dynamic(str("=")),
        dynamic(re(r"\s+").ignore()),
        dynamic(ref_("value")),
        dynamic(str(";")),
    ]);
    let grammar = GrammarBuilder::new()
        .rule("document", capture("lines", ref_("line").repeat(1, None)))
        .rule("line", line)
        .rule("value", value)
        .rule("ident", re("[a-zA-Z_][a-zA-Z0-9_]*"))
        .rule("number", re("[0-9]+"))
        .build();
    grammar.to_json().unwrap()
}

fn input_bytes(lines: usize) -> String {
    (0..lines)
        .map(|i| format!("ident_{} = value_{};\n", i, i * 7 % 1000))
        .collect()
}

fn from_json() -> Grammar {
    Grammar::from_json(&grammar_json()).unwrap()
}

fn bench_parse(c: &mut Criterion) {
    let grammar = from_json();
    let small = input_bytes(80);
    let large = input_bytes(2_500); // ~64 KB
    let mut truncated = large.clone();
    let trimmed_len = truncated.trim_end().len();
    truncated.truncate(trimmed_len); // last line loses its ";" → whole-parse backtrack

    let mut group = c.benchmark_group("large-input-memoization");
    for (name, input) in [
        ("parse_2kb", &small),
        ("parse_64kb", &large),
        ("parse_64kb_fail_at_end", &truncated),
    ] {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut arena = AstArena::for_input(black_box(input).len());
                let mut parser = PortableParser::new(&grammar, black_box(input), &mut arena);
                black_box(parser.parse().is_ok())
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
