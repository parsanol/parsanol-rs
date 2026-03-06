//! Backend Comparison Benchmarks
//!
//! Compares Packrat vs Bytecode VM backends across multiple grammars and input sizes.
//!
//! Run with: cargo bench --no-default-features --bench backend-comparison
//!
//! To run specific benchmarks:
//!   cargo bench --no-default-features --bench backend-comparison -- json
//!   cargo bench --no-default-features --bench backend-comparison -- calculator
//!   cargo bench --no-default-features --bench backend-comparison -- url

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use std::time::Duration;

use parsanol::portable::{
    backend::{BytecodeBackend, PackratBackend, ParsingBackend},
    infix::{Assoc, InfixBuilder},
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

// ============================================================================
// Grammar Builders
// ============================================================================

mod grammars {
    use super::*;

    /// Simple JSON-like grammar (no nested structures for basic benchmark)
    pub fn json_simple() -> Grammar {
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

    /// Calculator grammar with precedence
    pub fn calculator() -> Grammar {
        let mut builder = GrammarBuilder::new();
        builder = builder.rule("expr", re(r"[0-9]+"));
        builder = builder.rule("number", re(r"[0-9]+"));
        builder = builder.rule(
            "primary",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(str("(")),
                    dynamic(ref_("expr")),
                    dynamic(str(")")),
                ])),
                dynamic(ref_("number")),
            ]),
        );
        let expr_atom = InfixBuilder::new()
            .primary(ref_("primary"))
            .op("*", 2, Assoc::Left)
            .op("/", 2, Assoc::Left)
            .op("+", 1, Assoc::Left)
            .op("-", 1, Assoc::Left)
            .build(&mut builder);
        builder.update_rule("expr", expr_atom);
        builder.build()
    }

    /// URL grammar
    pub fn url() -> Grammar {
        GrammarBuilder::new()
            .rule(
                "url",
                seq(vec![
                    dynamic(str("https://")),
                    dynamic(re(r"[a-zA-Z0-9.-]+")),
                    dynamic(str("/")),
                    dynamic(re(r"[a-zA-Z0-9./_-]*")),
                ]),
            )
            .build()
    }

    /// Email grammar
    pub fn email() -> Grammar {
        GrammarBuilder::new()
            .rule(
                "email",
                seq(vec![
                    dynamic(re(r"[a-zA-Z0-9._%+-]+")),
                    dynamic(str("@")),
                    dynamic(re(r"[a-zA-Z0-9.-]+")),
                    dynamic(str(".")),
                    dynamic(re(r"[a-zA-Z]{2,}")),
                ]),
            )
            .build()
    }

    /// Simple string literal grammar
    pub fn string_literal() -> Grammar {
        GrammarBuilder::new()
            .rule("string", re(r#""([^"\\]|\\.)*""#))
            .build()
    }

    /// S-expression grammar (nested)
    pub fn sexp() -> Grammar {
        // Note: This has nested repetition, so should use Packrat
        GrammarBuilder::new()
            .rule(
                "sexp",
                choice(vec![dynamic(ref_("atom")), dynamic(ref_("list"))]),
            )
            .rule("atom", re(r"[a-zA-Z0-9+-/*=<>!?:_]+"))
            .rule(
                "list",
                seq(vec![
                    dynamic(str("(")),
                    dynamic(re(r"[ \t\n]*")),
                    dynamic(re(r"(sexp[ \t\n]*)*")),
                    dynamic(str(")")),
                ]),
            )
            .build()
    }

    /// Balanced parentheses (tests nested repetition)
    pub fn balanced_parens() -> Grammar {
        GrammarBuilder::new()
            .rule(
                "balanced",
                choice(vec![
                    dynamic(str("")),
                    dynamic(seq(vec![
                        dynamic(str("(")),
                        dynamic(ref_("balanced")),
                        dynamic(str(")")),
                        dynamic(ref_("balanced")),
                    ])),
                ]),
            )
            .build()
    }

    /// ISO-8601 date grammar
    pub fn iso_date() -> Grammar {
        GrammarBuilder::new()
            .rule(
                "date",
                seq(vec![
                    dynamic(re(r"[0-9]{4}")), // year
                    dynamic(str("-")),
                    dynamic(re(r"[0-9]{2}")), // month
                    dynamic(str("-")),
                    dynamic(re(r"[0-9]{2}")), // day
                ]),
            )
            .build()
    }

    /// IP address grammar
    pub fn ip_address() -> Grammar {
        GrammarBuilder::new()
            .rule(
                "ip",
                seq(vec![
                    dynamic(re(r"[0-9]{1,3}")),
                    dynamic(str(".")),
                    dynamic(re(r"[0-9]{1,3}")),
                    dynamic(str(".")),
                    dynamic(re(r"[0-9]{1,3}")),
                    dynamic(str(".")),
                    dynamic(re(r"[0-9]{1,3}")),
                ]),
            )
            .build()
    }
}

// ============================================================================
// Test Data
// ============================================================================

mod data {
    // JSON data
    #[allow(dead_code)]
    pub fn json_tiny() -> &'static str {
        "null"
    }

    #[allow(dead_code)]
    pub fn json_small() -> &'static str {
        r#"{"name":"test","value":42}"#
    }

    pub fn json_values() -> Vec<&'static str> {
        vec!["true", "false", "null", "42", "-3.14", r#""hello world""#]
    }

    // Calculator data
    #[allow(dead_code)]
    pub fn calc_simple() -> &'static str {
        "42"
    }

    #[allow(dead_code)]
    pub fn calc_medium() -> &'static str {
        "1+2*3"
    }

    pub fn calc_complex() -> &'static str {
        "((1+2)*(3+4)+(5+6)*(7+8))*((9+10)*(11+12)+(13+14)*(15+16))"
    }

    pub fn calc_expressions() -> Vec<&'static str> {
        vec![
            "42",
            "1+2",
            "1+2*3",
            "(1+2)*3",
            "1+2*3/4-5",
            "((1+2)*(3+4))",
        ]
    }

    // URL data
    pub fn url_simple() -> &'static str {
        "https://example.com/"
    }

    pub fn url_path() -> &'static str {
        "https://api.example.com/v1/users/123/posts/456"
    }

    // Email data
    pub fn email_simple() -> &'static str {
        "test@example.com"
    }

    pub fn email_complex() -> &'static str {
        "user.name+tag@subdomain.example.co.uk"
    }

    // String literals
    pub fn string_simple() -> &'static str {
        r#""hello world""#
    }

    pub fn string_escaped() -> &'static str {
        r#""hello\nworld\twith\"escapes\"""#
    }

    // S-expressions
    pub fn sexp_simple() -> &'static str {
        "(+ 1 2)"
    }

    pub fn sexp_nested() -> &'static str {
        "(* (+ 1 2) (- 4 3))"
    }

    pub fn sexp_deep() -> &'static str {
        "(+ (+ (+ (+ 1 2) 3) 4) 5)"
    }

    // Balanced parens
    pub fn parens_simple() -> &'static str {
        "(())"
    }

    pub fn parens_nested() -> &'static str {
        "(((())))"
    }

    pub fn parens_complex() -> &'static str {
        "(()(()())(()))"
    }

    // ISO dates
    pub fn date_simple() -> &'static str {
        "2024-01-15"
    }

    // IP addresses
    pub fn ip_simple() -> &'static str {
        "192.168.1.1"
    }
}

// ============================================================================
// Benchmark Functions
// ============================================================================

fn bench_json(c: &mut Criterion) {
    let grammar = grammars::json_simple();
    let mut group = c.benchmark_group("json");

    for input in data::json_values() {
        group.throughput(Throughput::Bytes(input.len() as u64));

        // Packrat backend
        group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
            b.iter(|| {
                let mut backend = PackratBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });

        // Bytecode backend
        group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
            b.iter(|| {
                let mut backend = BytecodeBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });
    }

    group.finish();
}

fn bench_calculator(c: &mut Criterion) {
    let grammar = grammars::calculator();
    let mut group = c.benchmark_group("calculator");

    for input in data::calc_expressions() {
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
            b.iter(|| {
                let mut backend = PackratBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });

        group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
            b.iter(|| {
                let mut backend = BytecodeBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });
    }

    group.finish();
}

fn bench_url(c: &mut Criterion) {
    let grammar = grammars::url();
    let mut group = c.benchmark_group("url");

    let inputs = vec![data::url_simple(), data::url_path()];

    for input in inputs {
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
            b.iter(|| {
                let mut backend = PackratBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });

        group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
            b.iter(|| {
                let mut backend = BytecodeBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });
    }

    group.finish();
}

fn bench_email(c: &mut Criterion) {
    let grammar = grammars::email();
    let mut group = c.benchmark_group("email");

    let inputs = vec![data::email_simple(), data::email_complex()];

    for input in inputs {
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
            b.iter(|| {
                let mut backend = PackratBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });

        group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
            b.iter(|| {
                let mut backend = BytecodeBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });
    }

    group.finish();
}

fn bench_string_literal(c: &mut Criterion) {
    let grammar = grammars::string_literal();
    let mut group = c.benchmark_group("string_literal");

    let inputs = vec![data::string_simple(), data::string_escaped()];

    for input in inputs {
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
            b.iter(|| {
                let mut backend = PackratBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });

        group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
            b.iter(|| {
                let mut backend = BytecodeBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });
    }

    group.finish();
}

fn bench_sexp(c: &mut Criterion) {
    let grammar = grammars::sexp();
    let mut group = c.benchmark_group("sexp");

    // Note: This grammar has nested repetition, so Bytecode may be slower
    let inputs = vec![data::sexp_simple(), data::sexp_nested(), data::sexp_deep()];

    for input in inputs {
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
            b.iter(|| {
                let mut backend = PackratBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });

        group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
            b.iter(|| {
                let mut backend = BytecodeBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });
    }

    group.finish();
}

fn bench_balanced_parens(c: &mut Criterion) {
    let grammar = grammars::balanced_parens();
    let mut group = c.benchmark_group("balanced_parens");

    // This grammar has nested recursion - critical test for Packrat vs Bytecode
    let inputs = vec![
        data::parens_simple(),
        data::parens_nested(),
        data::parens_complex(),
    ];

    for input in inputs {
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
            b.iter(|| {
                let mut backend = PackratBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });

        group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
            b.iter(|| {
                let mut backend = BytecodeBackend::new();
                let _ = black_box(backend.parse(&grammar, input));
            })
        });
    }

    group.finish();
}

fn bench_iso_date(c: &mut Criterion) {
    let grammar = grammars::iso_date();
    let mut group = c.benchmark_group("iso_date");

    let input = data::date_simple();
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
        b.iter(|| {
            let mut backend = PackratBackend::new();
            let _ = black_box(backend.parse(&grammar, input));
        })
    });

    group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
        b.iter(|| {
            let mut backend = BytecodeBackend::new();
            let _ = black_box(backend.parse(&grammar, input));
        })
    });

    group.finish();
}

fn bench_ip_address(c: &mut Criterion) {
    let grammar = grammars::ip_address();
    let mut group = c.benchmark_group("ip_address");

    let input = data::ip_simple();
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_with_input(BenchmarkId::new("packrat", input), input, |b, input| {
        b.iter(|| {
            let mut backend = PackratBackend::new();
            let _ = black_box(backend.parse(&grammar, input));
        })
    });

    group.bench_with_input(BenchmarkId::new("bytecode", input), input, |b, input| {
        b.iter(|| {
            let mut backend = BytecodeBackend::new();
            let _ = black_box(backend.parse(&grammar, input));
        })
    });

    group.finish();
}

/// Throughput benchmark - parsing larger inputs
fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    // Generate larger JSON input
    let json_large = format!(
        "[{}]",
        (0..100)
            .map(|i| format!(r#"{{"id":{},"name":"item{}","active":true}}"#, i, i))
            .collect::<Vec<_>>()
            .join(",")
    );

    // Use simple grammar for throughput test
    let grammar = grammars::json_simple();

    group.throughput(Throughput::Bytes(json_large.len() as u64));

    group.bench_function("packrat_json_large", |b| {
        b.iter(|| {
            let mut backend = PackratBackend::new();
            let _ = black_box(backend.parse(&grammar, &json_large));
        })
    });

    group.bench_function("bytecode_json_large", |b| {
        b.iter(|| {
            let mut backend = BytecodeBackend::new();
            let _ = black_box(backend.parse(&grammar, &json_large));
        })
    });

    group.finish();
}

/// Summary benchmark - all backends on same input
fn bench_summary(c: &mut Criterion) {
    let mut group = c.benchmark_group("summary");
    group.measurement_time(Duration::from_secs(3));

    let grammar = grammars::calculator();
    let input = data::calc_complex();

    group.throughput(Throughput::Bytes(input.len() as u64));

    // Packrat
    group.bench_function("packrat_calc_complex", |b| {
        b.iter(|| {
            let mut backend = PackratBackend::new();
            let _ = black_box(backend.parse(&grammar, input));
        })
    });

    // Bytecode
    group.bench_function("bytecode_calc_complex", |b| {
        b.iter(|| {
            let mut backend = BytecodeBackend::new();
            let _ = black_box(backend.parse(&grammar, input));
        })
    });

    // Legacy PortableParser (for reference)
    group.bench_function("legacy_portable_calc_complex", |b| {
        b.iter(|| {
            let mut arena = AstArena::for_input(input.len());
            let mut parser = PortableParser::new(&grammar, input, &mut arena);
            let _ = black_box(parser.parse());
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_json,
    bench_calculator,
    bench_url,
    bench_email,
    bench_string_literal,
    bench_sexp,
    bench_balanced_parens,
    bench_iso_date,
    bench_ip_address,
    bench_throughput,
    bench_summary,
);
criterion_main!(benches);
