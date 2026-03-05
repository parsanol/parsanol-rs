//! Benchmarks comparing Bytecode VM vs Packrat backends
//!
//! This benchmark suite compares the two parsing backends:
//! - Packrat: Memoization-based parser (default)
//! - Bytecode: Stack-based VM with optimization passes
//!
//! Run with: cargo bench --bench bytecode

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, str, seq, GrammarBuilder},
    bytecode::{Backend, Parser},
    AstArena, PortableParser, Grammar,
};

// ============================================================================
// Grammar Builders
// ============================================================================

/// Build a simple JSON-like grammar
fn build_json_grammar() -> Grammar {
    GrammarBuilder::new()
        .rule(
            "value",
            choice(vec![
                dynamic(str("true")),
                dynamic(str("false")),
                dynamic(str("null")),
                dynamic(re(r#"-?[0-9]+(\.[0-9]+)?"#)),
                dynamic(re(r#""[^"]*""#)),
            ]),
        )
        .build()
}

/// Build an arithmetic expression grammar
fn build_calc_grammar() -> Grammar {
    GrammarBuilder::new()
        .rule("number", re(r"[0-9]+"))
        .rule(
            "expr",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(re(r"[0-9]+")),
                    dynamic(re(r"[+\-*/]")),
                    dynamic(re(r"[0-9]+")),
                ])),
                dynamic(re(r"[0-9]+")),
            ]),
        )
        .build()
}

/// Build an identifier grammar
fn build_identifier_grammar() -> Grammar {
    GrammarBuilder::new()
        .rule("identifier", re(r"[a-zA-Z_][a-zA-Z0-9_]*"))
        .build()
}

/// Build a whitespace grammar
fn build_whitespace_grammar() -> Grammar {
    GrammarBuilder::new()
        .rule("whitespace", re(r"[ \t\n\r]+"))
        .build()
}

// ============================================================================
// Packrat Backend Benchmarks
// ============================================================================

fn bench_packrat_json(c: &mut Criterion) {
    let grammar = build_json_grammar();
    let inputs = [
        ("null", "null"),
        ("bool", "true"),
        ("number", "42"),
        ("negative", "-123"),
        ("float", "3.14"),
        ("string", r#""hello""#),
    ];

    let mut group = c.benchmark_group("packrat_json");

    for (name, input) in &inputs {
        group.bench_function(*name, |b| {
            b.iter(|| {
                let mut arena = AstArena::for_input(input.len());
                let mut parser = PortableParser::new(&grammar, *input, &mut arena);
                black_box(parser.parse())
            })
        });
    }

    group.finish();
}

fn bench_packrat_calc(c: &mut Criterion) {
    let grammar = build_calc_grammar();
    let inputs = [
        ("simple", "42"),
        ("addition", "1+2"),
        ("subtraction", "10-5"),
        ("multiplication", "3*4"),
        ("division", "12/4"),
    ];

    let mut group = c.benchmark_group("packrat_calc");

    for (name, input) in &inputs {
        group.bench_function(*name, |b| {
            b.iter(|| {
                let mut arena = AstArena::for_input(input.len());
                let mut parser = PortableParser::new(&grammar, *input, &mut arena);
                black_box(parser.parse())
            })
        });
    }

    group.finish();
}

// ============================================================================
// Bytecode Backend Benchmarks
// ============================================================================

fn bench_bytecode_json(c: &mut Criterion) {
    let grammar = build_json_grammar();
    let inputs = [
        ("null", "null"),
        ("bool", "true"),
        ("number", "42"),
        ("negative", "-123"),
        ("float", "3.14"),
        ("string", r#""hello""#),
    ];

    let mut group = c.benchmark_group("bytecode_json");

    // Pre-compile the bytecode
    let mut parser = Parser::new(grammar.clone(), Backend::Bytecode);

    for (name, input) in &inputs {
        group.bench_function(*name, |b| {
            b.iter(|| {
                black_box(parser.parse(black_box(*input)))
            })
        });
    }

    group.finish();
}

fn bench_bytecode_calc(c: &mut Criterion) {
    let grammar = build_calc_grammar();
    let inputs = [
        ("simple", "42"),
        ("addition", "1+2"),
        ("subtraction", "10-5"),
        ("multiplication", "3*4"),
        ("division", "12/4"),
    ];

    let mut group = c.benchmark_group("bytecode_calc");

    // Pre-compile the bytecode
    let mut parser = Parser::new(grammar.clone(), Backend::Bytecode);

    for (name, input) in &inputs {
        group.bench_function(*name, |b| {
            b.iter(|| {
                black_box(parser.parse(black_box(*input)))
            })
        });
    }

    group.finish();
}

// ============================================================================
// Comparison Benchmarks (Side-by-Side)
// ============================================================================

fn bench_comparison_simple(c: &mut Criterion) {
    let grammar = build_identifier_grammar();
    let input = "hello_world_123";

    let mut group = c.benchmark_group("comparison_identifier");

    group.bench_function("packrat", |b| {
        b.iter(|| {
            let mut arena = AstArena::for_input(input.len());
            let mut parser = PortableParser::new(&grammar, input, &mut arena);
            black_box(parser.parse())
        })
    });

    // Pre-compile bytecode parser
    let mut bytecode_parser = Parser::new(grammar.clone(), Backend::Bytecode);
    group.bench_function("bytecode", |b| {
        b.iter(|| {
            black_box(bytecode_parser.parse(black_box(input)))
        })
    });

    group.finish();
}

fn bench_comparison_whitespace(c: &mut Criterion) {
    let grammar = build_whitespace_grammar();
    let input = "   \t\n\r   ";

    let mut group = c.benchmark_group("comparison_whitespace");

    group.bench_function("packrat", |b| {
        b.iter(|| {
            let mut arena = AstArena::for_input(input.len());
            let mut parser = PortableParser::new(&grammar, input, &mut arena);
            black_box(parser.parse())
        })
    });

    // Pre-compile bytecode parser
    let mut bytecode_parser = Parser::new(grammar.clone(), Backend::Bytecode);
    group.bench_function("bytecode", |b| {
        b.iter(|| {
            black_box(bytecode_parser.parse(black_box(input)))
        })
    });

    group.finish();
}

fn bench_comparison_json_values(c: &mut Criterion) {
    let grammar = build_json_grammar();

    let mut group = c.benchmark_group("comparison_json");

    // Test different JSON values
    let test_cases = [
        ("null", "null"),
        ("bool", "true"),
        ("number", "42"),
        ("string", r#""hello world""#),
    ];

    // Pre-compile bytecode parser
    let mut bytecode_parser = Parser::new(grammar.clone(), Backend::Bytecode);

    for (name, input) in &test_cases {
        group.bench_function(&format!("packrat_{}", name), |b| {
            b.iter(|| {
                let mut arena = AstArena::for_input(input.len());
                let mut parser = PortableParser::new(&grammar, *input, &mut arena);
                black_box(parser.parse())
            })
        });

        group.bench_function(&format!("bytecode_{}", name), |b| {
            b.iter(|| {
                black_box(bytecode_parser.parse(black_box(*input)))
            })
        });
    }

    group.finish();
}

// ============================================================================
// Repeated Parsing (Compilation Overhead)
// ============================================================================

fn bench_repeated_parsing(c: &mut Criterion) {
    let grammar = build_json_grammar();
    let input = "42";

    let mut group = c.benchmark_group("repeated_parsing");

    // Packrat - no compilation needed
    group.bench_function("packrat", |b| {
        b.iter(|| {
            let mut arena = AstArena::for_input(input.len());
            let mut parser = PortableParser::new(&grammar, input, &mut arena);
            black_box(parser.parse())
        })
    });

    // Bytecode - includes compilation overhead (new parser each time)
    group.bench_function("bytecode_with_compile", |b| {
        b.iter(|| {
            let mut parser = Parser::new(grammar.clone(), Backend::Bytecode);
            black_box(parser.parse(black_box(input)))
        })
    });

    // Bytecode - pre-compiled (reuse parser)
    let mut bytecode_parser = Parser::new(grammar.clone(), Backend::Bytecode);
    group.bench_function("bytecode_precompiled", |b| {
        b.iter(|| {
            black_box(bytecode_parser.parse(black_box(input)))
        })
    });

    group.finish();
}

// ============================================================================
// Large Input Benchmarks
// ============================================================================

fn bench_large_input(c: &mut Criterion) {
    let grammar = build_identifier_grammar();

    // Generate a long identifier
    let long_input: String = "abc_".repeat(100);

    let mut group = c.benchmark_group("large_input");

    group.bench_function("packrat", |b| {
        b.iter(|| {
            let mut arena = AstArena::for_input(long_input.len());
            let mut parser = PortableParser::new(&grammar, &long_input, &mut arena);
            black_box(parser.parse())
        })
    });

    // Pre-compile bytecode parser
    let mut bytecode_parser = Parser::new(grammar.clone(), Backend::Bytecode);
    group.bench_function("bytecode", |b| {
        b.iter(|| {
            black_box(bytecode_parser.parse(black_box(&long_input)))
        })
    });

    group.finish();
}

// ============================================================================
// Benchmark Groups
// ============================================================================

criterion_group!(
    packrat_benches,
    bench_packrat_json,
    bench_packrat_calc,
);

criterion_group!(
    bytecode_benches,
    bench_bytecode_json,
    bench_bytecode_calc,
);

criterion_group!(
    comparison_benches,
    bench_comparison_simple,
    bench_comparison_whitespace,
    bench_comparison_json_values,
    bench_repeated_parsing,
    bench_large_input,
);

criterion_main!(packrat_benches, bytecode_benches, comparison_benches);
